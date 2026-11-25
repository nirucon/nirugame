//! Box art from RetroArch's thumbnail folders:
//! `<thumbnails>/<playlist>/Named_Boxarts/<label>.png`. Images are decoded
//! and scaled down on a worker thread; the window asks only for the rows it
//! shows and never waits for a file.

use crate::systems;
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    sync::Arc,
};

/// Premultiplied-free ARGB pixels (0xAARRGGBB), row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

/// The files that may hold the box art of an entry, best match first: the
/// label in its playlist's folder, then the file name, then the same names
/// in the system's own RetroArch folder.
pub fn candidates(thumb_dirs: &[PathBuf], folders: &[&str], names: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in thumb_dirs {
        for folder in folders.iter().filter(|f| !f.is_empty()) {
            for name in names.iter().filter(|n| !n.is_empty()) {
                let p = dir
                    .join(folder)
                    .join("Named_Boxarts")
                    .join(format!("{}.png", systems::thumbnail_name(name)));
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// Decode a PNG and scale it to fit `max_w` x `max_h` (never up).
pub fn load(path: &Path, max_w: i32, max_h: i32) -> Option<Image> {
    let file = std::fs::File::open(path).ok()?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    // Box art is a few hundred pixels; refuse absurd sizes before allocating.
    let mut reader = dec.read_info().ok()?;
    {
        let info = reader.info();
        if info.width == 0 || info.height == 0 || info.width > 8192 || info.height > 8192 {
            return None;
        }
    }
    let mut buf = vec![0; reader.output_buffer_size()];
    let out = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (out.width as usize, out.height as usize);
    let ch = match out.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let stride = out.line_size;
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        let row = &buf[y * stride..y * stride + w * ch];
        for p in row.chunks_exact(ch) {
            let (r, g, b, a) = match ch {
                1 => (p[0], p[0], p[0], 255),
                2 => (p[0], p[0], p[0], p[1]),
                3 => (p[0], p[1], p[2], 255),
                _ => (p[0], p[1], p[2], p[3]),
            };
            px.push((a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32);
        }
    }
    Some(fit(
        &Image {
            w: w as i32,
            h: h as i32,
            px,
        },
        max_w,
        max_h,
    ))
}

/// Scale down with an area average so small box art stays readable.
pub fn fit(img: &Image, max_w: i32, max_h: i32) -> Image {
    let s = (max_w as f32 / img.w as f32)
        .min(max_h as f32 / img.h as f32)
        .min(1.0);
    let w = ((img.w as f32 * s).round() as i32).max(1);
    let h = ((img.h as f32 * s).round() as i32).max(1);
    if w == img.w && h == img.h {
        return img.clone();
    }
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let y0 = (y as i64 * img.h as i64 / h as i64) as i32;
        let y1 = (((y + 1) as i64 * img.h as i64 / h as i64) as i32).max(y0 + 1);
        for x in 0..w {
            let x0 = (x as i64 * img.w as i64 / w as i64) as i32;
            let x1 = (((x + 1) as i64 * img.w as i64 / w as i64) as i32).max(x0 + 1);
            let mut sum = [0u32; 4];
            let mut n = 0u32;
            for sy in y0..y1 {
                let row = (sy * img.w) as usize;
                for &p in &img.px[row + x0 as usize..row + x1 as usize] {
                    let a = p >> 24;
                    // Weigh colour by alpha so transparent edges do not darken.
                    sum[0] += a;
                    sum[1] += ((p >> 16) & 0xff) * a;
                    sum[2] += ((p >> 8) & 0xff) * a;
                    sum[3] += (p & 0xff) * a;
                    n += 1;
                }
            }
            let a = sum[0] / n;
            let c = |v: u32| v.checked_div(sum[0]).unwrap_or(0);
            px.push(a << 24 | c(sum[1]) << 16 | c(sum[2]) << 8 | c(sum[3]));
        }
    }
    Image { w, h, px }
}

/// A request for one entry at one size.
pub struct Request {
    pub id: u64,
    pub files: Vec<PathBuf>,
    pub w: i32,
    pub h: i32,
}

pub enum Reply {
    Ready(u64, Arc<Image>),
    Missing(u64),
    /// Skipped because newer requests came first; ask again when visible.
    Dropped(u64),
}

/// Requests waiting at once; older ones are dropped while scrolling fast.
const QUEUE: usize = 48;

/// Start the loader thread. `wake` runs after each batch of replies.
pub fn spawn(wake: impl Fn() + Send + 'static) -> (Sender<Request>, Receiver<Reply>) {
    let (tx, rx) = mpsc::channel::<Request>();
    let (rtx, rrx) = mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("nirugame-thumbs".into())
        .spawn(move || {
            let mut queue: Vec<Request> = Vec::new();
            loop {
                if queue.is_empty() {
                    match rx.recv() {
                        Ok(r) => queue.push(r),
                        Err(_) => return,
                    }
                }
                queue.extend(rx.try_iter());
                if queue.len() > QUEUE {
                    for old in queue.drain(..queue.len() - QUEUE) {
                        let _ = rtx.send(Reply::Dropped(old.id));
                    }
                }
                // Newest first: that is what is on screen now.
                let Some(req) = queue.pop() else {
                    continue;
                };
                let img = req
                    .files
                    .iter()
                    .find(|f| f.is_file())
                    .and_then(|f| load(f, req.w, req.h));
                let reply = match img {
                    Some(i) => Reply::Ready(req.id, Arc::new(i)),
                    None => Reply::Missing(req.id),
                };
                if rtx.send(reply).is_err() {
                    return;
                }
                wake();
            }
        });
    (tx, rrx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_follow_retroarch_layout_and_rules() {
        let dirs = [PathBuf::from("/t")];
        let c = candidates(
            &dirs,
            &["SNES", "Nintendo - Super Nintendo Entertainment System"],
            &["Tom & Jerry (USA)", "Tom & Jerry (USA)", "tomjerry"],
        );
        assert_eq!(
            c[0],
            PathBuf::from("/t/SNES/Named_Boxarts/Tom _ Jerry (USA).png")
        );
        assert_eq!(c[1], PathBuf::from("/t/SNES/Named_Boxarts/tomjerry.png"));
        assert_eq!(
            c[2],
            PathBuf::from("/t/Nintendo - Super Nintendo Entertainment System/Named_Boxarts/Tom _ Jerry (USA).png")
        );
        assert_eq!(c.len(), 4);
    }

    fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(rgba).unwrap();
    }

    #[test]
    fn decodes_and_scales_png() {
        let dir = std::env::temp_dir().join(format!("nirugame-png-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("box.png");
        // 4x2: left half red, right half blue, all opaque.
        let mut px = Vec::new();
        for _y in 0..2 {
            for x in 0..4 {
                px.extend(if x < 2 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                });
            }
        }
        write_png(&path, 4, 2, &px);
        let full = load(&path, 100, 100).unwrap();
        assert_eq!((full.w, full.h), (4, 2));
        assert_eq!(full.px[0], 0xffff0000);
        assert_eq!(full.px[3], 0xff0000ff);
        let half = load(&path, 2, 2).unwrap();
        assert_eq!((half.w, half.h), (2, 1));
        assert_eq!(half.px, vec![0xffff0000, 0xff0000ff]);
        std::fs::write(&path, b"not a png").unwrap();
        assert!(load(&path, 10, 10).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn transparent_pixels_do_not_darken_the_average() {
        let img = Image {
            w: 2,
            h: 1,
            px: vec![0x00000000, 0xffffffff],
        };
        let one = fit(&img, 1, 1);
        assert_eq!(one.px[0], 0x7fffffff);
    }

    #[test]
    fn loader_replies_ready_or_missing() {
        let dir = std::env::temp_dir().join(format!("nirugame-loader-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("a.png");
        write_png(&path, 1, 1, &[1, 2, 3, 255]);
        let (tx, rx) = spawn(|| {});
        tx.send(Request {
            id: 1,
            files: vec![dir.join("none.png"), path.clone()],
            w: 8,
            h: 8,
        })
        .unwrap();
        tx.send(Request {
            id: 2,
            files: vec![dir.join("none.png")],
            w: 8,
            h: 8,
        })
        .unwrap();
        let mut ready = false;
        let mut missing = false;
        for _ in 0..2 {
            match rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
                Reply::Ready(1, img) => ready = img.px[0] == 0xff010203,
                Reply::Missing(2) => missing = true,
                _ => {}
            }
        }
        assert!(ready && missing);
        let _ = std::fs::remove_dir_all(dir);
    }
}
