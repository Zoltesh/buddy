---

# Buddy Design System

**Application Name:** Buddy
**Core Aesthetic:** Soft Tech.
**Design Principles:**

1. **Friendly but Efficient:** Approachable aesthetics (rounded corners, soft colors) applied to a high-density, professional layout.
2. **Dark Mode Native:** Designed specifically for prolonged usage in low-light environments.
3. **Data-First:** Visual hierarchy prioritizes content, code, and metrics over decorative elements.

---

## 1. Typography

### Logo & Brand

* **Font:** `Fredoka One`
* **Usage:** Exclusively for the main "buddy" application logo.
* **Color:** Sky Teal (`#00ADB5`).

### Interface & Body

* **Font:** `Nunito`
* **Usage:** General UI, navigation, buttons, and chat content.
* **Weights:**
* **Bold (700):** Section headers (e.g., "DASHBOARD", "MEMORY USAGE").
* **SemiBold (600):** Interactive elements (Buttons, Nav Links).
* **Regular (400):** Body text, chat messages, input text.



### Technical & Monospace

* **Font:** `JetBrains Mono` (Preferred) or `Fira Code`.
* **Usage:** Code snippets, terminal logs, API keys, and dashboard metric values.

---

## 2. Color Palette

The scheme is "Midnight & Magma"—a deep blue-grey dark mode with vibrant teal and rust accents.

| Role | Color Name | Hex | Usage Guide |
| --- | --- | --- | --- |
| **App Background** | **Gunmetal** | `#222831` | Global application background. |
| **Sidebar** | **Deep Gunmetal** | `#1F232B` | Navigation rail background. Slightly darker than the main content area. |
| **Surface** | **Charcoal** | `#393E46` | Card backgrounds, input fields, and modal windows. Use with `bg-opacity` for depth. |
| **Primary** | **Sky Teal** | `#00ADB5` | Active states, icons, thin borders, focus rings, and the Buddy avatar. |
| **Accent** | **Rust Magma** | `#FF5722` | Destructive actions, "Stop" buttons, notification badges, and graph trend lines. |
| **Text (Primary)** | **Cloud** | `#EEEEEE` | Main content text. |
| **Text (Secondary)** | **Ash** | `#ADB5BD` | Meta-data, timestamps, placeholders, and inactive labels. |

---

## 3. Shape & Layout

**Corner Radius Strategy:**

* **Cards & Containers:** `rounded-xl` (Standard 12px-16px).
* **Interactive Elements:** `rounded-lg` (Standard 8px-10px) for buttons and inputs.
* **Avatars:** `rounded-xl` (Soft square) or Circular.

**Spacing & Density:**

* **Grid Gap:** Compact (`gap-2` to `gap-4`).
* **Padding:** Comfortable but not sparse (`p-4` for cards, `px-4 py-2` for buttons).

---

## 4. UI Components

### Navigation Rail (Sidebar)

* **Container:** Fixed width, `Deep Gunmetal` background.
* **Item State (Inactive):** Text in `Ash`, no background.
* **Item State (Active):** Text in `White`, 3px vertical border on the left in `Sky Teal`.

### Dashboard Cards

* **Container:** `Charcoal` background with slight transparency (`bg-opacity-40`) and `backdrop-blur-sm`.
* **Border:** 1px solid `Charcoal` (`border-gray-700`).
* **Headers:** Uppercase, tracking-wide, small font size (`text-xs` or `text-sm`).
* **Metrics:** Large `JetBrains Mono` text for data values.

### Chat Interface

* **Input Bar:**
* Floating container at the bottom.
* Background: `Charcoal`.
* Border: None (Default) / 1px `Sky Teal` (Focus).
* Shape: `rounded-lg`.


* **Messages:**
* **User:** Background `Charcoal` (`bg-gray-700`), `rounded-lg`. Text in `Cloud`.
* **Buddy:** Transparent background. Icon avatar to the left. Text in `Cloud`.



### Buttons

* **Primary Action:** `Sky Teal` background, White text.
* **Destructive / High Attention:** `Rust Magma` background, White text.
* **Secondary / Ghost:** Transparent background, `Sky Teal` text.

---

## 5. Tailwind Configuration

Use the following configuration to strictly enforce the design system values.

```javascript
module.exports = {
  theme: {
    extend: {
      colors: {
        buddy: {
          dark: '#1F232B',    // Sidebar / Deep BG
          bg: '#222831',      // Main App BG
          surface: '#393E46', // Components / Cards
          teal: '#00ADB5',    // Primary Brand
          rust: '#FF5722',    // Accent / Action
          text: '#EEEEEE',    // Primary Text
          muted: '#ADB5BD',   // Secondary Text
        }
      },
      fontFamily: {
        brand: ['"Fredoka One"', 'cursive'],
        sans: ['"Nunito"', 'sans-serif'],
        mono: ['"JetBrains Mono"', 'monospace'],
      },
      borderRadius: {
        'card': '16px', // Standard container radius
        'btn': '10px',  // Standard interaction radius
      }
    }
  }
}

```

---