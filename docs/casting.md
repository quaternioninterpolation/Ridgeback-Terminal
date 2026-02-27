# Casting & Screen Sharing

Ridgeback supports sharing your terminal to external displays through both **native OS screen sharing** and **Google Cast (Chromecast)** streaming. Access these features via **Settings → Cast / Share**.

---

## Native Screen Sharing

Ridgeback's window is a standard OS-level surface, so it works out of the box with all native screen sharing mechanisms:

### macOS

- **AirPlay**: Open Control Center → Screen Mirroring → select your AirPlay display or Apple TV. You can mirror your entire screen or share just the Ridgeback window.
- **Screen Sharing app**: Use the built-in Screen Sharing utility to share your desktop (including Ridgeback) with another Mac on the same network.
- **Sidecar**: Extend your display to an iPad — Ridgeback will appear as a regular window on the extended desktop.
- **SharePlay / FaceTime screen share**: During a FaceTime call, click "Share My Screen" to let others see your terminal in real time.

### Windows

- **Miracast**: Press **Win+K** to open the Cast panel and stream to any Miracast-compatible wireless display or TV.
- **Connect app**: Use the built-in Connect app to receive or send screen content over Wi-Fi Direct.
- **Remote Desktop**: Ridgeback renders normally when accessed via RDP or similar remote desktop tools.

### Linux

- **PipeWire / Wayland screen capture**: Ridgeback works with PipeWire-based screen sharing used by browsers, Zoom, Teams, and OBS.
- **OBS Studio**: Capture the Ridgeback window using OBS's "Window Capture" or "Screen Capture" source.
- **X11 screen sharing**: On X11, standard tools like `xdg-desktop-portal` and `xdpyinfo` can capture the window.
- **VNC**: Remote desktop via TigerVNC, TurboVNC, or similar — Ridgeback renders as a regular window.

No configuration is needed — native sharing operates at the OS compositor level and works automatically.

---

## Google Cast (Chromecast)

Ridgeback includes built-in Google Cast device discovery, allowing you to find and stream to Chromecast, Google Home Hub, and Cast-enabled smart TVs on your local network.

### How It Works

1. Open **Settings → Cast / Share**
2. Click **"Scan for devices"** to discover Cast-enabled devices via SSDP (Simple Service Discovery Protocol)
3. Select a device from the list to initiate streaming
4. Click **"Stop"** to end the casting session

### Supported Devices

| Device | Type | Status |
|---|---|---|
| Chromecast (all generations) | Dongle | ✅ Discovered |
| Chromecast with Google TV | Dongle + OS | ✅ Discovered |
| Google Nest Hub / Hub Max | Smart display | ✅ Discovered |
| Android TV (Sony, TCL, etc.) | Smart TV | ✅ Discovered |
| Samsung Smart TV (Cast-enabled) | Smart TV | ✅ Discovered |
| LG webOS (Cast-enabled) | Smart TV | ✅ Discovered |

### Discovery Protocol

Ridgeback uses **SSDP (Simple Service Discovery Protocol)** to find DIAL-compatible devices:

```
M-SEARCH * HTTP/1.1
HOST: 239.255.255.250:1900
MAN: "ssdp:discover"
MX: 3
ST: urn:dial-multiscreen-org:service:dial:1
```

The search is broadcast to the SSDP multicast group `239.255.255.250:1900`. Responses are parsed for device names, types, and addresses.

### Network Requirements

- Ridgeback and the Cast device must be on the **same local network** (or VLAN with multicast routing)
- UDP port **1900** must be open for SSDP discovery
- Firewalls should allow outbound multicast to `239.255.255.250`

### Troubleshooting

| Issue | Solution |
|---|---|
| No devices found | Ensure you're on the same Wi-Fi network as the Cast device |
| Scan times out | Check firewall rules — SSDP requires UDP 1900 |
| Device shows but won't connect | The device may not support DIAL app launching |
| Casting is laggy | Reduce shader effects or lower `max_shader_fps` in rendering settings |

---

## Technical Architecture

```
┌──────────────────┐
│   Ridgeback      │
│   (egui window)  │
├──────────────────┤
│  CastManager     │
│  ├── SSDP scan   │──── UDP multicast ──── Cast devices
│  ├── Device list  │
│  └── Stream ctrl  │
└──────────────────┘
```

The `CastManager` runs device discovery on a background thread to avoid blocking the UI. Device scanning takes approximately 3 seconds (the SSDP MX timeout).

### Frame Streaming (Planned)

Future versions will support direct frame streaming to Cast devices using MJPEG over HTTP, bypassing the need for full Chrome Cast protocol (CASTV2). This will enable:

- Streaming the terminal viewport at configurable FPS
- Automatic quality reduction on slow connections
- Audio passthrough for terminal bells

---

## Privacy

- Device discovery is **local-only** — no data leaves your network
- No Google account or sign-in is required for Cast discovery
- Frame data is streamed directly to the device over your LAN, never through cloud servers
- Cast sessions can be stopped instantly from the Settings panel
