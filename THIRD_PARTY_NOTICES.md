# Third-party notices

Glance's original code is licensed under [MIT](LICENSE). Third-party code and
assets retain their own licenses; the project license does not replace them.

- **GPUI**, by Zed Industries, is licensed under Apache-2.0.
  `src/animation.rs` also adapts GPUI's Gaussian shadow integration from
  `src/platform/mac/shaders.metal`. See [source attribution](assets/gpui/SOURCE)
  and the [Apache-2.0 license](assets/gpui/LICENSE-APACHE).
- **Lucide icons** are licensed under ISC, with some icons derived from Feather
  under MIT. See [source attribution](assets/lucide/SOURCE) and the
  [complete icon license notices](assets/lucide/LICENSE).
- **The Glance app mark** comes from Modem's `agentpaste` project. Its source
  and revision are recorded in [the icon design notes](assets/icons/DESIGN.md).
- **Roboto Regular**, by Google, is embedded as an offline annotation fallback
  font under Apache-2.0. See [source attribution](assets/fonts/SOURCE) and the
  [font license](assets/fonts/LICENSE).

The app bundle includes this notice, Glance's license, and the GPUI, Lucide and Roboto
license files. Other Rust dependencies are recorded in `Cargo.lock`; their
upstream license terms apply to distributions that include them.
