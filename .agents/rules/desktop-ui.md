# UI Design Protocol

Guidelines for the Ferruginous Desktop Interface, balancing data transparency with premium operability.

## 1. Single Source of Truth (SSoT)
- **Rule**: Minimize internal UI state. Content MUST be projected directly from the `ferruginous-sdk` model.
- **Purpose**: Prevent desynchronization between the visual interface and the underlying PDF state.
- **Compliance**: UI widgets must consume SDK-native handles or structures.

## 2. Premium Visual Language
- **HSL Harmony**: Utilize a curated HSL color palette to ensure visual consistency across Light and Dark modes.
- **Micro-interactions**: Implement subtle 150-200ms transitions and hover effects to provide a responsive, high-end experience.
- **Typography**: Leverage high-precision fonts (Inter/Outfit) for numerical and code-heavy displays.

## 3. Layout Resilience
- **Dynamic Scaling**: All components MUST be resilient to viewport resizing and High-DPI scaling changes without layout breakage.
- **Interaction IDs**: Assign globally unique, descriptive IDs to all interactive elements to support automated testing and accessibility.

## 4. Visual Accessibility
- **Contrast Standard**: Maintain a minimum 4.5:1 contrast ratio for all legible text.
- **Theme Parity**: Ensure absolute functional and aesthetic parity between Light and Dark themes.
