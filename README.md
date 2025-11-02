# Terminal Video Player (ASCII)

This small Rust program uses `ffmpeg` (with `libcaca` support) to render video as ASCII/ANSI art in your terminal and optionally plays audio using `ffplay` or `afplay` (macOS).

Features
- Play video in terminal as ASCII art
- Configure FPS, width and height
- Optional audio playback (ffplay/afplay)

Requirements
- Rust toolchain (stable) for building the program
- `ffmpeg` available on PATH
  - The video ASCII renderer uses the `caca` output format. Your `ffmpeg` build must include `libcaca` for `-f caca` to work.
  - On macOS you can install ffmpeg via Homebrew: `brew install ffmpeg` (if the bottle does not include libcaca, you may need to build ffmpeg with libcaca support)
- `ffplay` (part of ffmpeg) is preferred for audio. On macOS, `afplay` is used as a fallback.

Usage

Build:

```bash
cargo build --release
```

Run (examples):

```bash
# Play video at 12 fps, width 100, auto height, with audio
./target/release/videoplayer myvideo.mp4 --fps 12 --width 100

# Play video without audio
./target/release/videoplayer myvideo.mp4 --no-sound --fps 10 --width 80

# Force a height in characters
./target/release/videoplayer myvideo.mp4 --fps 15 --width 120 --height 40
```

Notes and limitations
- This program shells out to `ffmpeg` for rendering ASCII frames. That keeps the Rust code simple but requires the external dependency.
- Audio sync may not be perfect because video and audio are played by two separate processes. This is a pragmatic, simple approach.
- If `ffmpeg -f caca` is not available on your system, you can try installing `libcaca` and rebuilding `ffmpeg`, or use an alternative approach (e.g., converting frames in Rust and rendering — more complex).

Next steps / improvements
- Implement a native decoder in Rust for better sync and fewer external dependencies.
- Add frame buffering and audio-sync logic.
- Support other output renderers (truecolor block characters, sixel, iTerm2 images) for higher-quality rendering on terminals that support them.

If you want, I can try to detect `libcaca` capability automatically and provide a fallback rendering path.
