// Remove Quick Look's square padding from an authored vector-board render.
// This is a static artifact renderer, not QUSOR application code.
import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

guard CommandLine.arguments.count == 5,
      let width = Int(CommandLine.arguments[3]),
      let height = Int(CommandLine.arguments[4]), width > 0, height > 0 else {
    fatalError("Expected input.png output.png width height")
}
let input = URL(fileURLWithPath: CommandLine.arguments[1])
let output = URL(fileURLWithPath: CommandLine.arguments[2])
guard let source = CGImageSourceCreateWithURL(input as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
      width <= image.width, height <= image.height,
      let cropped = image.cropping(to: CGRect(x: 0, y: 0, width: width, height: height)),
      let destination = CGImageDestinationCreateWithURL(output as CFURL, UTType.png.identifier as CFString, 1, nil) else {
    fatalError("Could not read or crop the rendered board")
}
CGImageDestinationAddImage(destination, cropped, nil)
guard CGImageDestinationFinalize(destination) else {
    fatalError("Could not write the board PNG")
}
