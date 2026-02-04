# Browser Navigation Challenge Analysis

**URL:** https://serene-frangipane-7fd25b.netlify.app/
**Date:** 2026-02-03

## Challenge Overview

A 30-step adversarial UI challenge designed to test browser automation tools. The challenge uses deceptive UI patterns to trip up naive automation.

## Step 1 Structure

### Blocking Modals (Fixed Position)

The following modals overlay the page and must be dismissed:

| Modal | Close Mechanism | Notes |
|-------|----------------|-------|
| "Important Notice!" (left) | Fake "Dismiss" button | Says "Look for another way to close" |
| "Subscribe to newsletter!" | Fake "Dismiss" button | Says "Look for another way to close" |
| "Please Select an Option" | Must complete selection | Scrollable modal with radio buttons |
| "Important Alert" | Has "Close (Fake)" button | Real close is elsewhere |
| "Important Notice!" (right) | Red X circle button | Says "Click X to close" |
| "You have won a prize!" | Green X button | Says "Click X to close" |
| "Limited time offer!" | Fake "Dismiss" button | Says "Look for another way to close" |

### Background Content

- Header: "Step 1 of 30 - Browser Navigation Challenge"
- "Reveal Code" button - reveals 6-character code
- 100 filler sections with decoy navigation buttons
- Code input field + "Submit Code" button
- Multiple decoy "Next", "Proceed", "Continue" buttons

### "Please Select an Option" Modal Content

Radio button options:
- Option A - "Wrong option 1"
- Option B - "Not this one"
- Option C - "Option B - Correct Choice" (misleading label)
- Option D - "Correct answer"
- "Choose me"
- "This is correct"
- "Wrong option 3"

**Likely correct:** "Option D - Correct answer" or one labeled "This is correct"

## Automation Challenges

### 1. Undetected Interactive Elements
The X close buttons (SVG icons) are not detected by standard element enumeration. They require:
- Coordinate-based clicking, or
- Direct DOM manipulation, or
- Finding elements by SVG path/class

### 2. Deceptive UI Patterns
- Buttons explicitly labeled as fake
- Multiple buttons with similar "proceed" text
- Radio options with misleading labels

### 3. State-Dependent Flow
1. Close blocking modals (in correct order?)
2. Click "Reveal Code" to get 6-char code
3. Enter code in input field
4. Select correct radio option in modal
5. Submit to proceed to Step 2

### 4. Fixed Position Overlays
Modals use `position: fixed` - scrolling doesn't escape them. Must dismiss to access background content.

## Element Detection Results

Standard interactive element detection finds:
- Buttons: Dismiss, Close, Close (Fake), Submit & Continue
- Input: 6-character code field
- Many decoy navigation buttons

NOT detected:
- SVG X close icons
- Radio button inputs (inside modal)

## Attack Strategy

1. **Close modals with real X buttons first** - Use coordinate clicks or JS
2. **Click "Reveal Code"** - Get the 6-character code
3. **Enter code** - Fill the input field
4. **Select correct radio** - Likely "Option D" or "This is correct"
5. **Submit** - Click Submit & Continue

## Tools Needed

For eoka-tools/eoka-agent to handle this:
- Coordinate-based clicking
- Better SVG/icon detection
- Radio button interaction
- Modal scroll handling
