# Bull Face Ideas (Target: 50)

Current: 28 faces. Need 22 more.

## New Face Ideas + Triggers

### Capture Reactions
| Face | Trigger | Description |
|------|---------|-------------|
| Triumphant | 4-way handshake captured | Horns down, charging pose |
| Smug | WPA3/rare capture | Sunglasses, got something good |
| Surprised | Unexpected PMKID | Wide eyes, didn't see that coming |

### Environmental
| Face | Trigger | Description |
|------|---------|-------------|
| Rainy | Low signal strength (RSSI < -80) | Wet bull, dripping |
| Sunny | Many APs visible (>20) | Happy grazing, strong signals |
| Night Owl | Time 10pm-2am | Bull with moon |

### Activity
| Face | Trigger | Description |
|------|---------|-------------|
| Eating | Processing captures | Munching on handshakes |
| Running | Drive-by mode / walkby capture | Bull sprinting |
| Sniffing | Scanning phase | Nose down, sniffing for APs |
| Headbutt | Deauth frame sent | Horns forward, impact |

### Social
| Face | Trigger | Description |
|------|---------|-------------|
| Waving | Peer discovered (pwngrid) | Friendly wave |
| Flexing | Level up milestone | Showing off muscles |
| Dancing | 100th capture | Party bull |

### System
| Face | Trigger | Description |
|------|---------|-------------|
| Sweating | CPU temp > 65°C | Overheating bull |
| Plugged In | PiSugar charging | Bull with power cable |
| Yawning | >30 idle epochs | Wide mouth, bored beyond belief |
| Dizzy | After firmware crash recovery | Stars around head |
| Skeptical | Whitelisted AP seen | One eyebrow raised |
| Thinking | Dictionary cracking running | Chin on hoof |
| Celebrating | Password cracked! | Confetti, party hat |
| Sneaking | Low-power / stealth scan | Tiptoeing bull |

## Art Specs
- Format: 1-bit PNG, black on white (or white on black if ui.invert=true)
- Size: varies by display area, face region is roughly 250x74 pixels
- Style: match existing 28 faces — hand-drawn, expressive, clear at low res
- File location: faces/eink/
- Naming: lowercase, matches Face enum variant (e.g., triumphant.png, smug.png)
