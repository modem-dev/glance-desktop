// Offline OCR, invoked only for a PNG staged in Glance's private temporary directory.
import Foundation
import Vision

do {
    guard CommandLine.arguments.count == 2 else {
        throw NSError(domain: "GlanceOCR", code: 1,
                      userInfo: [NSLocalizedDescriptionKey: "Expected a local PNG path"])
    }
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = false
    if #available(macOS 13.0, *) {
        request.automaticallyDetectsLanguage = true
    }
    let handler = VNImageRequestHandler(url: URL(fileURLWithPath: CommandLine.arguments[1]), options: [:])
    try handler.perform([request])
    // Preserve Vision's reading order and line boundaries. No inferred table/code formatting.
    let text = (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
    guard text.utf8.count <= 65_536 else {
        throw NSError(domain: "GlanceOCR", code: 2,
                      userInfo: [NSLocalizedDescriptionKey: "Recognized text exceeds 64 KiB; extract a smaller area"])
    }
    FileHandle.standardOutput.write(Data(text.utf8))
} catch {
    FileHandle.standardError.write(Data(error.localizedDescription.utf8))
    exit(1)
}
