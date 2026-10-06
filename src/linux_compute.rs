//! Worker-only Vulkan compute with cached pipelines/buffers and bounded readback.
//! GPUI still uploads the resulting RGBA image; this is not zero-copy presentation.
//! Scalar storage parameters and packed pixels have four-byte strides (std430).
//! Each effect caches one mutex-protected pipeline/buffer set, used only by workers.
//! Avoid thread-local GPU resources: wgpu 28 debug lock tracing can panic when
//! resources drop after its tracing TLS has already been destroyed.
use std::{borrow::Cow, sync::Arc, time::Duration};
use wgpu::util::DeviceExt;

struct Device {
    device: wgpu::Device,
    queue: wgpu::Queue,
}
fn device() -> Result<Arc<Device>, String> {
    static DEVICE: std::sync::OnceLock<Result<Arc<Device>, String>> = std::sync::OnceLock::new();
    DEVICE
        .get_or_init(|| {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::VULKAN,
                ..Default::default()
            });
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                }))
                .map_err(|e| e.to_string())?;
            let info = adapter.get_info();
            if info.device_type == wgpu::DeviceType::Cpu {
                return Err(format!("{} is a software Vulkan adapter", info.name));
            }
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("Glance animation compute"),
                    required_limits: adapter.limits(),
                    ..Default::default()
                }))
                .map_err(|e| e.to_string())?;
            eprintln!("Glance animation GPU: {} ({:?})", info.name, info.backend);
            Ok(Arc::new(Device { device, queue }))
        })
        .clone()
}

pub(crate) struct Compute {
    gpu: Arc<Device>,
    pipeline: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    buffers: Option<Buffers>,
    source: Option<(Arc<image::RgbaImage>, wgpu::Buffer)>,
    failure: Option<String>,
}
struct Buffers {
    output: wgpu::Buffer,
    readback: wgpu::Buffer,
    bindings: wgpu::BindGroup,
    size: u64,
}
impl Compute {
    pub(crate) fn new(shader: &'static str, words: usize) -> Result<Self, String> {
        let gpu = device()?;
        let oom = gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = gpu.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Glance compute shader"),
                source: wgpu::ShaderSource::Glsl {
                    shader: Cow::Borrowed(shader),
                    stage: wgpu::naga::ShaderStage::Compute,
                    defines: Default::default(),
                },
            });
        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Glance compute pipeline"),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let errors = [
            pollster::block_on(scope.pop()),
            pollster::block_on(internal.pop()),
            pollster::block_on(oom.pop()),
        ];
        if let Some(error) = errors.into_iter().flatten().next() {
            return Err(error.to_string());
        }
        let params = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glance compute parameters"),
            size: words as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            gpu,
            pipeline,
            params,
            buffers: None,
            source: None,
            failure: None,
        })
    }

    pub(crate) fn frame(
        &mut self,
        width: u32,
        height: u32,
        params: &[u32],
        source: Option<&Arc<image::RgbaImage>>,
    ) -> Result<image::RgbaImage, String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        self.validate_frame(width, height, params, source)?;
        let oom = self
            .gpu
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = self
            .gpu
            .device
            .push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = self
            .gpu
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let mut result = self.render(width, height, params, source);
        let errors = [
            pollster::block_on(validation.pop()),
            pollster::block_on(internal.pop()),
            pollster::block_on(oom.pop()),
        ];
        if let Some(error) = errors.into_iter().flatten().next() {
            result = Err(error.to_string());
        }
        if let Err(error) = &result {
            // Do not retry a broken device (or wait ten seconds) on every frame.
            self.failure = Some(error.clone());
            self.buffers = None;
            self.source = None;
        }
        result
    }

    fn validate_frame(
        &self,
        width: u32,
        height: u32,
        params: &[u32],
        source: Option<&Arc<image::RgbaImage>>,
    ) -> Result<(), String> {
        let size = (u64::from(width) * u64::from(height))
            .checked_mul(4)
            .ok_or("Compute frame byte size overflows")?;
        let limits = self.gpu.device.limits();
        if size == 0
            || size > u64::from(limits.max_storage_buffer_binding_size)
            || width.div_ceil(16) > limits.max_compute_workgroups_per_dimension
            || height.div_ceil(16) > limits.max_compute_workgroups_per_dimension
            || params.len() as u64 * 4 != self.params.size()
            || params.first() != Some(&width)
            || params.get(1) != Some(&height)
        {
            return Err("Compute frame exceeds device limits or has invalid parameters".into());
        }
        if source.is_some_and(|source| source.dimensions() != (width, height)) {
            return Err("Compute source dimensions do not match output".into());
        }
        Ok(())
    }

    fn render(
        &mut self,
        width: u32,
        height: u32,
        params: &[u32],
        source: Option<&Arc<image::RgbaImage>>,
    ) -> Result<image::RgbaImage, String> {
        let size = u64::from(width) * u64::from(height) * 4;
        if let Some(source) = source
            && self
                .source
                .as_ref()
                .is_none_or(|(old, _)| !Arc::ptr_eq(old, source))
        {
            self.source = Some((
                source.clone(),
                self.gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Glance entrance source"),
                        contents: source.as_raw(),
                        usage: wgpu::BufferUsages::STORAGE,
                    }),
            ));
            self.buffers = None;
        }
        if self.buffers.as_ref().is_none_or(|b| b.size != size) {
            let make = |label, usage| {
                self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage,
                    mapped_at_creation: false,
                })
            };
            let output = make(
                "Glance compute output",
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            );
            let readback = make(
                "Glance compute readback",
                wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            );
            let mut entries = vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.params.as_entire_binding(),
                },
            ];
            if let Some((_, source)) = &self.source {
                entries.push(wgpu::BindGroupEntry {
                    binding: 2,
                    resource: source.as_entire_binding(),
                });
            }
            let bindings = self
                .gpu
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Glance compute bindings"),
                    layout: &self.pipeline.get_bind_group_layout(0),
                    entries: &entries,
                });
            self.buffers = Some(Buffers {
                output,
                readback,
                bindings,
                size,
            });
        }
        let b = self.buffers.as_ref().unwrap();
        self.gpu
            .queue
            .write_buffer(&self.params, 0, bytemuck::cast_slice(params));
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &b.bindings, &[]);
            pass.dispatch_workgroups(width.div_ceil(16), height.div_ceil(16), 1);
        }
        encoder.copy_buffer_to_buffer(&b.output, 0, &b.readback, 0, size);
        let submission = self.gpu.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        b.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        if let Err(e) = self.gpu.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        }) {
            b.readback.unmap();
            return Err(e.to_string());
        }
        let mapped = rx
            .recv_timeout(Duration::from_secs(1))
            .map_err(|e| e.to_string())
            .and_then(|result| result.map_err(|e| e.to_string()));
        if let Err(error) = mapped {
            b.readback.unmap();
            return Err(error);
        }
        let pixels = b.readback.slice(..).get_mapped_range().to_vec();
        b.readback.unmap();
        image::RgbaImage::from_raw(width, height, pixels).ok_or("Invalid compute output".into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires hardware Vulkan; synthetic buffers only, no files/network"]
    fn vulkan_rejects_invalid_frames_without_disabling_valid_work() {
        let mut shader = super::Compute::new(include_str!("shaders/motion.comp"), 20).unwrap();
        let mut params = [0; 20];
        params[0] = 17;
        params[1] = 19;
        params[3] = 1_f32.to_bits();
        assert!(shader.frame(0, 0, &params, None).is_err());
        assert!(shader.frame(17, 19, &params[..19], None).is_err());
        assert!(shader.frame(u32::MAX, u32::MAX, &params, None).is_err());
        let source = std::sync::Arc::new(image::RgbaImage::new(1, 1));
        assert!(shader.frame(17, 19, &params, Some(&source)).is_err());
        let frame = shader.frame(17, 19, &params, None).unwrap();
        assert_eq!(frame.dimensions(), (17, 19));
        assert!(frame.pixels().all(|p| p[3] == 255));
    }

    #[test]
    fn vulkan_shaders_validate_without_a_device() {
        use wgpu::naga;
        for source in [
            include_str!("shaders/motion.comp"),
            include_str!("shaders/entrance.comp"),
        ] {
            let module = naga::front::glsl::Frontend::default()
                .parse(
                    &naga::front::glsl::Options::from(naga::ShaderStage::Compute),
                    source,
                )
                .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
        }
    }
}
