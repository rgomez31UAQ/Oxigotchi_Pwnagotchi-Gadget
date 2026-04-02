# Mood Interaction Buttons — Design Spec

## Goal

Add interactive mood-boost buttons to the web dashboard so owners can keep their bull's mood around 80% through casual interaction, while requiring real walks (handshake captures) to reach 100%.

## Architecture

Three buttons (Pet, Treat, Praise) placed below the e-ink preview image inside the Live Display card. Each button has an independent 5-minute cooldown tracked server-side using monotonic time (`Instant`). Mood boosts are soft-capped at 0.8 — the closer to 0.8, the less effect buttons have.

## Interaction Design

### Buttons

| Button | Boost | Cooldown |
|--------|-------|----------|
| Pet    | +0.03 | 5 min    |
| Treat  | +0.05 | 5 min    |
| Praise | +0.04 | 5 min    |

### Soft Cap Formula

```
effective_boost = base_boost * max(0.0, 1.0 - (current_mood / 0.8))
```

- At mood 0.0: full boost (1.0x multiplier)
- At mood 0.4: half boost (0.5x multiplier)
- At mood 0.8+: zero boost (0.0x multiplier)
- Only real XP from handshakes/walks pushes mood past 0.8

### Cooldown

- Tracked per action type using `Instant::now()` (monotonic, no RTC/NTP dependency)
- 5-minute cooldown per button, independent of each other
- Server rejects requests during cooldown, returns seconds remaining
- Cooldown state lives in `DaemonState`, not persisted to disk (resets on service restart — intentional, not worth persisting)

## API

### POST /api/interact

**Request:**
```json
{ "action": "pet" | "treat" | "praise" }
```

**Response (success):**
```json
{ "ok": true, "message": "Pet! +3% mood", "cooldown_secs": 300 }
```

**Response (cooldown active):**
```json
{ "ok": false, "message": "Pet on cooldown", "cooldown_secs": 187 }
```

**Response (at cap):**
```json
{ "ok": true, "message": "Pet! Mood already near cap", "cooldown_secs": 300 }
```

## Backend Changes

### DaemonState (web/mod.rs)

New fields:
```rust
pub interact_cooldowns: HashMap<String, Instant>,  // "pet" -> last interaction time
pub pending_mood_boost: Option<f32>,                // queued boost for daemon
```

### interact_handler (web/mod.rs)

New handler following the optimistic update pattern:
1. Lock shared state
2. Check cooldown — reject if < 300s since last interaction of this type
3. Compute effective boost using soft cap formula
4. Apply mood boost immediately to `state.mood` (optimistic, clamped to 0.0..=1.0)
5. Queue `pending_mood_boost` for daemon to apply to real personality state
6. Update cooldown timestamp
7. Return response with cooldown_secs

### Daemon::process_web_commands (main.rs)

Add processing for `pending_mood_boost`:
- Add `boost` to `self.personality.mood`, clamp to 0.0..=1.0
- Save XP stats (mood is persisted in exp_stats.json)
- Clear the pending boost

## Frontend Changes

### Live Display card (html.rs)

Add below the e-ink image `<div>`, inside the same card:

```html
<div id="interact-btns" style="margin-top:8px;display:flex;gap:6px;justify-content:center">
  <button class="btn interact-btn" data-action="pet" onclick="interact('pet')">Pet</button>
  <button class="btn interact-btn" data-action="treat" onclick="interact('treat')">Treat</button>
  <button class="btn interact-btn" data-action="praise" onclick="interact('praise')">Praise</button>
</div>
```

Buttons use existing `.btn` class styling. During cooldown: button disabled, shows countdown text (e.g., "Pet 4:32"), re-enables when cooldown expires.

### JavaScript (html.rs)

```javascript
function interact(action) {
    var btn = document.querySelector('[data-action="'+action+'"]');
    btn.disabled = true;
    api('POST', '/api/interact', { action: action }).then(function(d) {
        if (!d) { btn.disabled = false; return; }
        startCooldown(btn, action, d.cooldown_secs);
        if (d.ok) refreshPersonality();  // update mood bar immediately
    });
}

function startCooldown(btn, action, secs) {
    var label = action.charAt(0).toUpperCase() + action.slice(1);
    var end = Date.now() + secs * 1000;
    var iv = setInterval(function() {
        var left = Math.max(0, Math.round((end - Date.now()) / 1000));
        if (left <= 0) { clearInterval(iv); btn.textContent = label; btn.disabled = false; return; }
        var m = Math.floor(left/60), s = left%60;
        btn.textContent = label + ' ' + m + ':' + (s<10?'0':'') + s;
    }, 1000);
}
```

### Initial Cooldown State

On page load, fetch cooldown state from the status or personality endpoint (add `interact_cooldowns` to the response) so buttons show correct state on refresh. Alternatively, add a `GET /api/interact` that returns current cooldowns for all three actions.

## Route Registration

```rust
.route("/api/interact", get(interact_status_handler).post(interact_handler))
```

## Testing

- Unit test: soft cap formula at 0.0, 0.4, 0.8, 1.0 mood levels
- Unit test: cooldown rejection (mock Instant)
- Unit test: optimistic mood update clamps to 0.0..=1.0
- Unit test: pending_mood_boost processed by daemon
- Integration: button tap -> mood increases -> personality API reflects change

## What This Does NOT Change

- Mood decay rate (already halved in v3.2, holds 1.0 through the day after a walk)
- XP system (real handshakes still the only way past 0.8)
- Display rendering (face selection based on mood thresholds unchanged)
- exp_stats.json format (mood field already exists as f32)
