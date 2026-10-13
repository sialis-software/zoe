// Make the macOS icon set from the artwork: the picture on a rounded square with the margin the
// system's own icons have, at every size an .icns holds.
// Usage: swift scripts/make-icon.swift <artwork.png> <out.iconset>
import AppKit

let arguments = CommandLine.arguments
guard arguments.count == 3, let artwork = NSImage(contentsOfFile: arguments[1]) else {
    FileHandle.standardError.write(Data("usage: make-icon.swift <artwork.png> <out.iconset>\n".utf8))
    exit(1)
}
let out = URL(fileURLWithPath: arguments[2])
try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)

func png(_ pixels: Int) -> Data {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels, bitsPerSample: 8, samplesPerPixel: 4,
        hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let side = CGFloat(pixels)
    let inset = side * 100 / 1024
    let square = NSRect(x: inset, y: inset, width: side - 2 * inset, height: side - 2 * inset)
    let radius = square.width * 0.2237
    NSBezierPath(roundedRect: square, xRadius: radius, yRadius: radius).addClip()
    artwork.draw(in: square, from: .zero, operation: .sourceOver, fraction: 1)
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

for (name, pixels) in [
    ("icon_16x16", 16), ("icon_16x16@2x", 32), ("icon_32x32", 32), ("icon_32x32@2x", 64),
    ("icon_128x128", 128), ("icon_128x128@2x", 256), ("icon_256x256", 256), ("icon_256x256@2x", 512),
    ("icon_512x512", 512), ("icon_512x512@2x", 1024),
] {
    try png(pixels).write(to: out.appendingPathComponent("\(name).png"))
}
