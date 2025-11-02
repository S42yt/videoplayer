use anyhow::{bail, Context, Result};
use clap::Parser;
use std::io::{self, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Parser, Debug)]
#[command(author, version, about = "Play videos in the terminal (ASCII) with optional sound", long_about = None)]
struct Args {
    input: String,

    #[arg(long, default_value_t = 24)]
    fps: u32,

    #[arg(long, default_value_t = 80)]
    width: u32,

    #[arg(long, default_value_t = 70)]
    height: i32,

    #[arg(long = "no-sound", default_value_t = false)]
    no_sound: bool,
    
    #[arg(long = "no-color", default_value_t = false)]
    no_color: bool,
}

fn find_program(names: &[&str]) -> Option<String> {
    for &n in names {
        if which::which(n).is_ok() {
            return Some(n.to_string());
        }
    }
    None
}

fn spawn_audio_player(input: &str) -> Result<Child> {
    if let Some(prog) = find_program(&["afplay"]) {
        let child = Command::new(prog)
            .arg(input)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn afplay for audio playback")?;
        return Ok(child);
    }

    bail!("afplay not found. Install afplay (macOS) or modify the code to use a different audio player.");
}

fn probe_video_size(path: &str) -> Option<(u32, u32)> {
    if which::which("ffprobe").is_err() {
        return None;
    }

    let out = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height")
        .arg("-of")
        .arg("csv=p=0:s=x")
        .arg(path)
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let s = String::from_utf8_lossy(&out.stdout);
    let s = s.trim();
    let mut parts = s.split('x');
    if let (Some(w), Some(h)) = (parts.next(), parts.next()) {
        if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
            return Some((w, h));
        }
    }
    None
}

fn spawn_ffmpeg_raw(args: &Args, target_w: u32, target_h: u32) -> Result<Child> {
    let vf = format!("fps={},scale={}:{}", args.fps, target_w, target_h);

    let child = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-nostdin")
        .arg("-i")
        .arg(&args.input)
        .arg("-an")
        .arg("-vf")
        .arg(&vf)
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgb24")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to spawn ffmpeg for rawvideo output. Is ffmpeg installed?")?;

    Ok(child)
}

fn render_ascii_frame(buf: &[u8], w: u32, h: u32) -> String {
    const CHARS: &[u8] = b"@%#*+=-:. ";
    let mut out = String::with_capacity((w as usize + 1) * h as usize);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let idx = (y * w as usize + x) * 3;
            if idx + 2 >= buf.len() {
                out.push(' ');
                continue;
            }
            let r = buf[idx] as f32;
            let g = buf[idx + 1] as f32;
            let b = buf[idx + 2] as f32;
            let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let t = (lum / 255.0) * ((CHARS.len() - 1) as f32);
            let ch = CHARS[(CHARS.len() - 1 - t as usize).min(CHARS.len() - 1)];
            out.push(ch as char);
        }
        if y != (h as usize - 1) {
            out.push('\n');
        }
    }
    out
}

fn render_color_frame(buf: &[u8], w: u32, h: u32) -> String {
    let mut out = String::with_capacity((w as usize * 8 + 1) * h as usize);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let idx = (y * w as usize + x) * 3;
            if idx + 2 >= buf.len() {
                out.push(' ');
                continue;
            }
            let r = buf[idx];
            let g = buf[idx + 1];
            let b = buf[idx + 2];
            out.push_str(&format!("\x1b[48;2;{};{};{}m  ", r, g, b));
        }
        out.push_str("\x1b[0m");
        if y != (h as usize - 1) {
            out.push('\n');
        }
    }
    out
}

fn main() -> Result<()> {
    let args = Args::parse();

    if which::which("ffmpeg").is_err() {
        bail!("ffmpeg not found. Install ffmpeg and ensure it is on PATH.");
    }

    println!(
        "Playing {}  (fps={} width={} height={} sound={})",
        args.input, args.fps, args.width, args.height, !args.no_sound
    );

    let render_w = if args.no_color { args.width } else { (args.width.max(1) / 2).max(1) };

    let target_h = if args.height > 0 {
        args.height as u32
    } else {
        if let Some((src_w, src_h)) = probe_video_size(&args.input) {
            let mut h = (src_h as f32 * render_w as f32 / src_w as f32) as u32;

            h = (h as f32 * 0.55).max(1.0) as u32;
            h
        } else {
            (render_w as f32 * 9.0 / 16.0) as u32
        }
    };

    let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(vec![]));

    if !args.no_sound {
        let input_clone = args.input.clone();
        let cp = child_pids.clone();
        thread::spawn(move || {
            match spawn_audio_player(&input_clone) {
                Ok(mut ch) => {
                    let pid = ch.id();
                    if let Ok(mut lock) = cp.lock() {
                        lock.push(pid);
                    }
                    let _ = ch.wait();
                }
                Err(e) => {
                    eprintln!("Warning: couldn't start audio: {e}");
                }
            }
        });
    }

    let mut ff = spawn_ffmpeg_raw(&args, render_w, target_h)?;

    if let Ok(mut lock) = child_pids.lock() {
        lock.push(ff.id());
    }
    let cp = child_pids.clone();
    ctrlc::set_handler(move || {
        eprintln!("Stopping playback...");
        if let Ok(lock) = cp.lock() {
            for pid in lock.iter() {
                let _ = std::process::Command::new("kill")
                    .arg("-9")
                    .arg(pid.to_string())
                    .spawn();
            }
        }
        std::process::exit(0);
    })
    .context("Failed to set Ctrl-C handler")?;

    let stdout = ff.stdout.take().context("ffmpeg stdout not captured")?;
    let mut reader = io::BufReader::new(stdout);

    let frame_size = (render_w as usize) * (target_h as usize) * 3;

    print!("\x1b[2J\x1b[H\x1b[?25l");
    io::stdout().flush().ok();

    let frame_duration = Duration::from_secs_f32(1.0 / args.fps as f32);

    let mut buf = vec![0u8; frame_size];

    loop {
        let start = Instant::now();

        if let Err(e) = reader.read_exact(&mut buf) {
            eprintln!("Finished reading frames or error: {e}");
            break;
        }

        let out = if args.no_color {
            render_ascii_frame(&buf, args.width, target_h)
        } else {
            render_color_frame(&buf, render_w, target_h)
        };

        print!("\x1b[H");
        print!("{}", out);
        io::stdout().flush().ok();

        let elapsed = start.elapsed();
        if elapsed < frame_duration {
            thread::sleep(frame_duration - elapsed);
        }
    }

    print!("\x1b[?25h\n");

    if let Ok(lock) = child_pids.lock() {
        for pid in lock.iter() {
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).spawn();
        }
    }

    let _ = ff.wait();

    Ok(())
}
