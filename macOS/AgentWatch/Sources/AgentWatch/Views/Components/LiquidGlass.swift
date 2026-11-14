import SwiftUI

// Liquid Glass helpers.
//
// On macOS 26+ these use the system Liquid Glass material, so they follow
// System Settings > Appearance > Liquid Glass (Clear / Tinted) and
// Accessibility > Display > Reduce Transparency automatically.
// On older systems they fall back to the previous ultra-thin material look.

extension View {
    /// Liquid Glass behind the view, clipped to `shape`.
    @ViewBuilder
    func liquidGlass<S: Shape>(in shape: S, interactive: Bool = false) -> some View {
        if #available(macOS 26.0, *) {
            self.glassEffect(interactive ? .regular.interactive() : .regular, in: shape)
        } else {
            self
                .background(.ultraThinMaterial)
                .clipShape(shape)
                .overlay(shape.stroke(Color.white.opacity(0.2), lineWidth: 0.5))
        }
    }
}
