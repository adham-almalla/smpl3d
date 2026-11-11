// Draws the Smpl3D app icon and logos.
// Run: swift make_brand.swift <output-folder>
import AppKit
import CoreGraphics

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."

func color(_ hex: UInt32, _ a: CGFloat = 1) -> CGColor {
  CGColor(
    red: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
    blue: CGFloat(hex & 0xFF) / 255, alpha: a)
}

func render(_ w: Int, _ h: Int, _ draw: (CGContext) -> Void) -> NSBitmapImageRep {
  let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: w, pixelsHigh: h, bitsPerSample: 8, samplesPerPixel: 4,
    hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
  let g = NSGraphicsContext(bitmapImageRep: rep)!
  NSGraphicsContext.saveGraphicsState()
  NSGraphicsContext.current = g
  let ctx = g.cgContext
  // y-down coordinates
  ctx.translateBy(x: 0, y: CGFloat(h))
  ctx.scaleBy(x: 1, y: -1)
  draw(ctx)
  NSGraphicsContext.restoreGraphicsState()
  return rep
}

func save(_ rep: NSBitmapImageRep, _ name: String) {
  let data = rep.representation(using: .png, properties: [:])!
  try! data.write(to: URL(fileURLWithPath: "\(out)/\(name)"))
}

func poly(_ ctx: CGContext, _ pts: [CGPoint]) -> CGPath {
  let p = CGMutablePath()
  p.addLines(between: pts)
  p.closeSubpath()
  return p
}

func fill(_ ctx: CGContext, _ path: CGPath, top: CGColor, bottom: CGColor, from: CGPoint, to: CGPoint) {
  ctx.saveGState()
  ctx.addPath(path)
  ctx.clip()
  let grad = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: [top, bottom] as CFArray, locations: [0, 1])!
  ctx.drawLinearGradient(grad, start: from, end: to, options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
  ctx.restoreGState()
}

/// Isometric cube with a drilled hole, centred at c, edge length s.
func drawCube(_ ctx: CGContext, c: CGPoint, s: CGFloat, shadow: Bool, outline: CGColor? = nil) {
  let ax = cos(.pi / 6) * s, ay = sin(.pi / 6) * s
  let top = CGPoint(x: c.x, y: c.y - s)
  let p0 = top
  let p1 = CGPoint(x: top.x + ax, y: top.y + ay)
  let p2 = CGPoint(x: top.x, y: top.y + 2 * ay)
  let p3 = CGPoint(x: top.x - ax, y: top.y + ay)
  let d = CGPoint(x: 0, y: s)
  func add(_ a: CGPoint, _ b: CGPoint) -> CGPoint { CGPoint(x: a.x + b.x, y: a.y + b.y) }

  if shadow {
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -s * 0.12), blur: s * 0.35, color: color(0x3A0010, 0.45))
    ctx.addPath(poly(ctx, [p0, p1, add(p1, d), add(p2, d), add(p3, d), p3]))
    ctx.setFillColor(color(0xFFFFFF))
    ctx.fillPath()
    ctx.restoreGState()
  }

  let left = poly(ctx, [p3, p2, add(p2, d), add(p3, d)])
  let right = poly(ctx, [p2, p1, add(p1, d), add(p2, d)])
  let topF = poly(ctx, [p0, p1, p2, p3])
  fill(ctx, left, top: color(0xFBF1EA), bottom: color(0xEAD6CA), from: p3, to: add(p2, d))
  fill(ctx, right, top: color(0xE6C9BA), bottom: color(0xCDA491), from: p2, to: add(p1, d))
  fill(ctx, topF, top: color(0xFFFFFF), bottom: color(0xFCF5F0), from: p0, to: p2)

  // Drilled hole in the top face.
  let hc = CGPoint(x: top.x, y: top.y + ay)
  let rx = ax * 0.62, ry = ay * 0.62
  let hole = CGPath(ellipseIn: CGRect(x: hc.x - rx, y: hc.y - ry, width: 2 * rx, height: 2 * ry), transform: nil)
  ctx.saveGState()
  ctx.addPath(hole)
  ctx.clip()
  fill(ctx, hole, top: color(0xB06A52), bottom: color(0xD9A68E), from: CGPoint(x: hc.x, y: hc.y - ry), to: CGPoint(x: hc.x, y: hc.y + ry))
  let depth = ry * 0.85
  let floor = CGPath(ellipseIn: CGRect(x: hc.x - rx, y: hc.y - ry + depth, width: 2 * rx, height: 2 * ry), transform: nil)
  fill(ctx, floor, top: color(0x9A2330), bottom: color(0x5E0F22), from: CGPoint(x: hc.x, y: hc.y), to: CGPoint(x: hc.x, y: hc.y + ry + depth))
  ctx.restoreGState()

  // Crisp edges.
  ctx.setLineJoin(.round)
  ctx.setLineWidth(max(1, s * 0.012))
  ctx.setStrokeColor(outline ?? color(0xFFFFFF, 0.9))
  ctx.addPath(topF)
  ctx.strokePath()
  ctx.setStrokeColor(color(0x5E0F22, 0.18))
  ctx.addPath(left)
  ctx.addPath(right)
  ctx.strokePath()
}

/// macOS squircle (continuous corners) in the standard icon grid.
func squircle(_ r: CGRect) -> CGPath {
  let p = CGMutablePath()
  let n: CGFloat = 5
  let cx = r.midX, cy = r.midY, a = r.width / 2, b = r.height / 2
  for i in 0...360 {
    let t = CGFloat(i) * .pi / 180
    let ct = cos(t), st = sin(t)
    let x = cx + a * (ct >= 0 ? 1 : -1) * pow(abs(ct), 2 / n)
    let y = cy + b * (st >= 0 ? 1 : -1) * pow(abs(st), 2 / n)
    i == 0 ? p.move(to: CGPoint(x: x, y: y)) : p.addLine(to: CGPoint(x: x, y: y))
  }
  p.closeSubpath()
  return p
}

// ---------------------------------------------------------------- app icon
let icon = render(1024, 1024) { ctx in
  let r = CGRect(x: 100, y: 100, width: 824, height: 824)
  let sq = squircle(r)
  ctx.saveGState()
  ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 28, color: color(0x000000, 0.28))
  ctx.addPath(sq)
  ctx.setFillColor(color(0xC2401F))
  ctx.fillPath()
  ctx.restoreGState()
  fill(ctx, sq, top: color(0xFF8A1F), bottom: color(0x7A1028), from: CGPoint(x: 512, y: 100), to: CGPoint(x: 512, y: 924))
  // soft top sheen
  ctx.saveGState()
  ctx.addPath(sq)
  ctx.clip()
  let sheen = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: [color(0xFFFFFF, 0.22), color(0xFFFFFF, 0)] as CFArray, locations: [0, 1])!
  ctx.drawRadialGradient(sheen, startCenter: CGPoint(x: 360, y: 180), startRadius: 0, endCenter: CGPoint(x: 360, y: 180), endRadius: 620, options: [])
  // faint sketch grid
  ctx.setStrokeColor(color(0xFFFFFF, 0.07))
  ctx.setLineWidth(2)
  var x: CGFloat = 100
  while x <= 924 { ctx.move(to: CGPoint(x: x, y: 100)); ctx.addLine(to: CGPoint(x: x, y: 924)); x += 68.7 }
  var y: CGFloat = 100
  while y <= 924 { ctx.move(to: CGPoint(x: 100, y: y)); ctx.addLine(to: CGPoint(x: 924, y: y)); y += 68.7 }
  ctx.strokePath()
  ctx.restoreGState()
  // inner hairline
  ctx.addPath(sq)
  ctx.setStrokeColor(color(0xFFFFFF, 0.18))
  ctx.setLineWidth(3)
  ctx.strokePath()
  drawCube(ctx, c: CGPoint(x: 512, y: 548), s: 270, shadow: true)
}
save(icon, "icon_1024.png")

// ---------------------------------------------------------------- mark (transparent)
let mark = render(256, 256) { ctx in
  drawCube(ctx, c: CGPoint(x: 128, y: 136), s: 104, shadow: false, outline: color(0xC2401F, 0.35))
}
save(mark, "logo_mark.png")

// Small rounded-square mark for the in-app title bar.
let badge = render(128, 128) { ctx in
  let sq = squircle(CGRect(x: 4, y: 4, width: 120, height: 120))
  fill(ctx, sq, top: color(0xFF8A1F), bottom: color(0x7A1028), from: CGPoint(x: 64, y: 4), to: CGPoint(x: 64, y: 124))
  drawCube(ctx, c: CGPoint(x: 64, y: 69), s: 40, shadow: false)
}
save(badge, "logo_badge.png")

// ---------------------------------------------------------------- wordmark logo
func wordmark(dark: Bool) -> NSBitmapImageRep {
  render(1400, 400) { ctx in
    let sq = squircle(CGRect(x: 40, y: 40, width: 320, height: 320))
    fill(ctx, sq, top: color(0xFF8A1F), bottom: color(0x7A1028), from: CGPoint(x: 200, y: 40), to: CGPoint(x: 200, y: 360))
    drawCube(ctx, c: CGPoint(x: 200, y: 212), s: 106, shadow: true)
    // Text (flip back for AppKit text drawing)
    ctx.saveGState()
    ctx.translateBy(x: 0, y: 400)
    ctx.scaleBy(x: 1, y: -1)
    let ink = dark ? NSColor.white : NSColor(red: 0.11, green: 0.11, blue: 0.12, alpha: 1)
    let blue = NSColor(red: 1.0, green: 0.50, blue: 0.12, alpha: 1)
    let font = NSFont.systemFont(ofSize: 190, weight: .bold)
    let s = NSMutableAttributedString(string: "Smpl", attributes: [.font: font, .foregroundColor: ink, .kern: -4])
    s.append(NSAttributedString(string: "3D", attributes: [.font: font, .foregroundColor: blue, .kern: -4]))
    s.draw(at: NSPoint(x: 420, y: 92))
    ctx.restoreGState()
  }
}
save(wordmark(dark: false), "logo.png")
save(wordmark(dark: true), "logo_dark.png")
print("done")
