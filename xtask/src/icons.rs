use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

pub const SOURCE_DIR: &str = "ui/src-tauri/icons/source";
pub const ICONS: &str = "ui/src-tauri/icons";
pub const HASHES: &str = "ui/src-tauri/icons/SOURCES.sha256";
const FAVICON: &str = "ui/public/favicon.svg";
const RETINA_DPI_PER_METRE: u32 = 5669;

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    if !args.is_empty() {
        return Err(super::USAGE.to_string());
    }
    let files = generate(repo)?;
    for (path, bytes) in &files {
        let path = repo.join(path);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    }
    Ok((0, format!("wrote {} brand assets from {SOURCE_DIR}", files.len())))
}

/// Every shipped brand asset, keyed by its repo-relative path.
pub fn generate(repo: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut out = BTreeMap::new();
    let mut put = |name: &str, bytes: Vec<u8>| out.insert(format!("{ICONS}/{name}"), bytes);

    let [s16, s24, s32] = sized(render(repo, "icon-small.svg", &[16, 24, 32])?)?;
    let [m48, m64, m128, m256, m512, m1024] =
        sized(render(repo, "icon.svg", &[48, 64, 128, 256, 512, 1024])?)?;
    put("32x32.png", png(&s32, None)?);
    put("128x128.png", png(&m128, None)?);
    put("128x128@2x.png", png(&m256, None)?);
    put("icon.png", png(&m512, None)?);
    put(
        "icon.icns",
        icns(&[
            (*b"icp4", png(&s16, None)?),
            (*b"icp5", png(&s32, None)?),
            (*b"ic11", png(&s32, None)?),
            (*b"ic12", png(&m64, None)?),
            (*b"ic07", png(&m128, None)?),
            (*b"ic13", png(&m256, None)?),
            (*b"ic08", png(&m256, None)?),
            (*b"ic14", png(&m512, None)?),
            (*b"ic09", png(&m512, None)?),
            (*b"ic10", png(&m1024, None)?),
        ]),
    );
    put("icon.ico", ico(&[&s16, &s24, &s32, &m48, &m64, &m256])?);

    for (source, name, width) in [
        ("tray-running-template.svg", "tray-running.png", 36),
        ("tray-stopped-template.svg", "tray-stopped.png", 36),
        ("tray-running.svg", "tray-running-color.png", 32),
        ("tray-stopped.svg", "tray-stopped-color.png", 32),
        ("social-preview.svg", "brand/social-preview.png", 1280),
        ("wordmark-light.svg", "brand/wordmark-light.png", 1480),
        ("wordmark-dark.svg", "brand/wordmark-dark.png", 1480),
    ] {
        put(name, png(&one(repo, source, width)?, None)?);
    }
    put(
        "installer/dmg-background.png",
        png(&one(repo, "dmg-background@2x.svg", 1320)?, Some(RETINA_DPI_PER_METRE))?,
    );
    for (source, width) in
        [("nsis-header", 150), ("nsis-sidebar", 164), ("wix-banner", 493), ("wix-dialog", 493)]
    {
        put(&format!("installer/{source}.bmp"), bmp(&one(repo, &format!("{source}.svg"), width)?));
    }

    put("SOURCES.sha256", source_hashes(repo)?.into_bytes());
    let favicon = std::fs::read(repo.join(SOURCE_DIR).join("favicon.svg"))
        .map_err(|e| format!("read favicon.svg: {e}"))?;
    out.insert(FAVICON.to_string(), favicon);
    Ok(out)
}

fn sized<const N: usize>(images: Vec<Image>) -> Result<[Image; N], String> {
    images.try_into().map_err(|_| "tauri icon returned the wrong number of sizes".to_string())
}

fn one(repo: &Path, source: &str, width: u32) -> Result<Image, String> {
    let [image] = sized(render(repo, source, &[width])?)?;
    Ok(image)
}

/// `sha256sum -c` format over every source file, sorted by name.
pub fn source_hashes(repo: &Path) -> Result<String, String> {
    let dir = repo.join(SOURCE_DIR);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    let mut sums = String::new();
    for name in names {
        let bytes = std::fs::read(dir.join(&name)).map_err(|e| format!("read {name}: {e}"))?;
        writeln!(sums, "{}  source/{name}", crate::git::sha256_hex(&bytes)?)
            .map_err(|e| e.to_string())?;
    }
    Ok(sums)
}

pub fn view_box(svg: &str) -> Option<[u32; 4]> {
    let root = &svg[svg.find("<svg")?..];
    let root = &root[..root.find('>')?];
    let start = root.find("viewBox=\"")? + "viewBox=\"".len();
    let numbers: Vec<u32> = root[start..]
        .split('"')
        .next()?
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    numbers.try_into().ok()
}

// A rectangular source is drawn top-left in a square wrapper, so `tauri icon`
// (square only) rasterises it 1:1 and the crop never resamples.
fn render(repo: &Path, source: &str, widths: &[u32]) -> Result<Vec<Image>, String> {
    let svg = std::fs::read_to_string(repo.join(SOURCE_DIR).join(source))
        .map_err(|e| format!("read {source}: {e}"))?;
    let Some([0, 0, vw @ 1..=u32::MAX, vh @ 1..=u32::MAX]) = view_box(&svg) else {
        return Err(format!("{source}: the viewBox must be integer, start at 0 0 and have a size"));
    };
    let body_start = svg.find("<svg").and_then(|i| svg[i..].find('>').map(|j| i + j + 1));
    let body_end = svg.rfind("</svg>");
    let (Some(start), Some(end)) = (body_start, body_end) else {
        return Err(format!("{source}: not an svg"));
    };
    let side = vw.max(vh);
    let wrapper = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{side}" height="{side}" viewBox="0 0 {side} {side}"><svg x="0" y="0" width="{vw}" height="{vh}" viewBox="0 0 {vw} {vh}">{}</svg></svg>"#,
        &svg[start..end]
    );
    let mut squares = Vec::new();
    for &width in widths {
        if (width * side) % vw != 0 || (width * vh) % vw != 0 {
            return Err(format!("{source}: {width} px is not a whole-pixel scale of {vw}x{vh}"));
        }
        squares.push(width * side / vw);
    }
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let input = scratch.path().join("source.svg");
    std::fs::write(&input, wrapper).map_err(|e| e.to_string())?;
    let sizes: Vec<String> = squares.iter().map(u32::to_string).collect();
    let pnpm = crate::package::Os::named(std::env::consts::OS)?.pnpm();
    let ran = Command::new(pnpm)
        .args(["exec", "tauri", "icon"])
        .arg(&input)
        .arg("-o")
        .arg(scratch.path())
        .args(["-p", &sizes.join(",")])
        .current_dir(repo.join("ui"))
        .output()
        .map_err(|e| format!("pnpm exec tauri icon: {e}"))?;
    if !ran.status.success() {
        return Err(format!(
            "tauri icon {source}: {}{}",
            String::from_utf8_lossy(&ran.stdout),
            String::from_utf8_lossy(&ran.stderr)
        ));
    }
    widths
        .iter()
        .zip(squares)
        .map(|(&width, square)| {
            let rendered = std::fs::read(scratch.path().join(format!("{square}x{square}.png")))
                .map_err(|e| e.to_string())?;
            Ok(crop(&decode_png(&rendered)?, width, width * vh / vw))
        })
        .collect()
}

pub fn decode_png(bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::ALPHA | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("png too large")?];
    let frame = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if frame.color_type != png::ColorType::Rgba {
        return Err(format!("expected RGBA, got {:?}", frame.color_type));
    }
    buf.truncate(frame.buffer_size());
    Ok(Image { width: frame.width, height: frame.height, rgba: buf })
}

fn crop(image: &Image, width: u32, height: u32) -> Image {
    let row = width as usize * 4;
    let stride = image.width as usize * 4;
    let rgba = (0..height as usize)
        .flat_map(|y| &image.rgba[y * stride..y * stride + row])
        .copied()
        .collect();
    Image { width, height, rgba }
}

fn png(image: &Image, dots_per_metre: Option<u32>) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_pixel_dims(dots_per_metre.map(|ppu| png::PixelDimensions {
        xppu: ppu,
        yppu: ppu,
        unit: png::Unit::Meter,
    }));
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&image.rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// 24-bit uncompressed BMP, bottom-up, alpha composited onto white.
#[allow(clippy::cast_possible_truncation)] // the format's fixed-width fields; a brand asset is far below 4 GiB
pub fn bmp(image: &Image) -> Vec<u8> {
    let row = (image.width as usize * 3).next_multiple_of(4);
    let pixels = row * image.height as usize;
    let mut out = Vec::with_capacity(54 + pixels);
    out.extend_from_slice(b"BM");
    for value in [54 + pixels as u32, 0, 54, 40, image.width, image.height] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    for value in [0, pixels as u32, 2835, 2835, 0, 0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for y in (0..image.height as usize).rev() {
        let start = out.len();
        for x in 0..image.width as usize {
            let at = (y * image.width as usize + x) * 4;
            let [r, g, b, a] = [0, 1, 2, 3].map(|i| u32::from(image.rgba[at + i]));
            let over_white = |c: u32| ((c * a + 255 * (255 - a) + 127) / 255) as u8;
            out.extend_from_slice(&[over_white(b), over_white(g), over_white(r)]);
        }
        out.resize(start + row, 0);
    }
    out
}

#[allow(clippy::cast_possible_truncation)] // the format's fixed-width fields; a brand asset is far below 4 GiB
pub fn ico(images: &[&Image]) -> Result<Vec<u8>, String> {
    let pngs = images.iter().map(|image| png(image, None)).collect::<Result<Vec<_>, _>>()?;
    let mut out = vec![0, 0, 1, 0];
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (image, data) in images.iter().zip(&pngs) {
        let side = |n: u32| if n >= 256 { 0 } else { n as u8 };
        out.extend_from_slice(&[side(image.width), side(image.height), 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for data in &pngs {
        out.extend_from_slice(data);
    }
    Ok(out)
}

#[allow(clippy::cast_possible_truncation)] // the format's fixed-width fields; a brand asset is far below 4 GiB
pub fn icns(entries: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let total = 8 + entries.iter().map(|(_, data)| 8 + data.len()).sum::<usize>();
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&(total as u32).to_be_bytes());
    for (kind, data) in entries {
        out.extend_from_slice(kind);
        out.extend_from_slice(&(8 + data.len() as u32).to_be_bytes());
        out.extend_from_slice(data);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> std::path::PathBuf {
        crate::repo_root().unwrap()
    }

    fn committed(path: &str) -> Vec<u8> {
        std::fs::read(repo().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn icon(name: &str) -> Image {
        decode_png(&committed(&format!("{ICONS}/{name}"))).unwrap()
    }

    fn le32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn le16(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    fn manifest() -> Vec<serde_json::Value> {
        let text = committed(&format!("{SOURCE_DIR}/MANIFEST.json"));
        let manifest: serde_json::Value = serde_json::from_slice(&text).unwrap();
        manifest["files"].as_array().unwrap().clone()
    }

    #[test]
    fn every_manifest_source_exists_with_its_view_box_and_plain_svg_only() {
        for row in manifest() {
            let file = row["file"].as_str().unwrap();
            let svg = String::from_utf8(committed(&format!("{SOURCE_DIR}/{file}"))).unwrap();
            let expected: Vec<u32> = row["viewBox"]
                .as_str()
                .unwrap()
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            assert_eq!(view_box(&svg).map(Vec::from), Some(expected), "{file}");
            for banned in ["var(", "<text", "href=\"http", "<image"] {
                assert!(!svg.contains(banned), "{file} carries {banned}");
            }
        }
    }

    #[test]
    fn the_recorded_source_hashes_are_the_sources_hashes() {
        assert!(
            String::from_utf8(committed(HASHES)).unwrap() == source_hashes(&repo()).unwrap(),
            "{HASHES} is stale against {SOURCE_DIR}: run cargo xtask icons"
        );
    }

    #[test]
    fn a_hidden_file_finder_leaves_in_the_source_folder_is_not_a_source() {
        let repo = tempfile::tempdir().unwrap();
        let dir = repo.path().join(SOURCE_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("icon.svg"), "<svg/>").unwrap();
        std::fs::write(dir.join(".DS_Store"), "finder").unwrap();
        let sums = source_hashes(repo.path()).unwrap();
        assert_eq!(sums.lines().count(), 1, "{sums}");
        assert!(sums.ends_with("  source/icon.svg\n"));
    }

    #[test]
    fn the_committed_brand_assets_are_the_ones_the_sources_generate() {
        let repo = repo();
        for (path, bytes) in generate(&repo).unwrap() {
            assert!(
                std::fs::read(repo.join(&path)).ok().as_ref() == Some(&bytes),
                "{path} is stale against {SOURCE_DIR}: run cargo xtask icons"
            );
        }
    }

    #[test]
    fn the_icon_set_has_the_sizes_its_bundle_entries_name() {
        for (name, side) in
            [("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256), ("icon.png", 512)]
        {
            let image = icon(name);
            assert_eq!((image.width, image.height), (side, side), "{name}");
        }
        let ico = committed(&format!("{ICONS}/icon.ico"));
        assert_eq!(&ico[..4], [0, 0, 1, 0]);
        let sides: Vec<u8> = (0..usize::from(le16(&ico, 4))).map(|i| ico[6 + 16 * i]).collect();
        assert_eq!(sides, [16, 24, 32, 48, 64, 0], "0 = 256");
        let icns = committed(&format!("{ICONS}/icon.icns"));
        assert_eq!(&icns[..4], b"icns");
        assert_eq!(u32::from_be_bytes(icns[4..8].try_into().unwrap()) as usize, icns.len());
        let mut kinds = Vec::new();
        let mut at = 8;
        while at < icns.len() {
            kinds.push(String::from_utf8(icns[at..at + 4].to_vec()).unwrap());
            at += u32::from_be_bytes(icns[at + 4..at + 8].try_into().unwrap()) as usize;
        }
        assert_eq!(
            kinds,
            ["icp4", "icp5", "ic11", "ic12", "ic07", "ic13", "ic08", "ic14", "ic09", "ic10"]
        );
    }

    #[test]
    fn the_macos_tray_pair_is_black_and_alpha_and_the_coloured_pair_is_not() {
        for name in ["tray-running.png", "tray-stopped.png"] {
            let glyph = icon(name);
            assert_eq!((glyph.width, glyph.height), (36, 36), "{name}");
            assert!(glyph.rgba.chunks(4).all(|px| px[..3] == [0, 0, 0]), "{name} is a template");
        }
        for name in ["tray-running-color.png", "tray-stopped-color.png"] {
            let glyph = icon(name);
            assert_eq!((glyph.width, glyph.height), (32, 32), "{name}");
            assert!(
                glyph.rgba.chunks(4).any(|px| px[3] > 0 && px[0] != px[2]),
                "{name} has colour"
            );
        }
        assert_ne!(icon("tray-running.png").rgba, icon("tray-stopped.png").rgba);
    }

    #[test]
    fn the_installer_art_has_the_exact_size_and_format_each_installer_takes() {
        for (name, width, height) in [
            ("nsis-header", 150, 57),
            ("nsis-sidebar", 164, 314),
            ("wix-banner", 493, 58),
            ("wix-dialog", 493, 312),
        ] {
            let bmp = committed(&format!("{ICONS}/installer/{name}.bmp"));
            assert_eq!(&bmp[..2], b"BM", "{name}");
            assert_eq!((le32(&bmp, 18), le32(&bmp, 22)), (width, height), "{name}");
            assert_eq!((le16(&bmp, 28), le32(&bmp, 30)), (24, 0), "{name}: 24-bit BI_RGB");
            assert_eq!(le32(&bmp, 2) as usize, bmp.len(), "{name}");
        }
        let dmg = committed(&format!("{ICONS}/installer/dmg-background.png"));
        let reader = png::Decoder::new(std::io::Cursor::new(&dmg)).read_info().unwrap();
        let info = reader.info();
        assert_eq!((info.width, info.height), (1320, 800));
        let dims = info.pixel_dims.unwrap();
        assert_eq!((dims.xppu, dims.yppu, dims.unit), (5669, 5669, png::Unit::Meter), "144 dpi");
    }

    #[test]
    fn the_repo_face_and_favicon_come_from_the_sources() {
        let social = icon("brand/social-preview.png");
        assert_eq!((social.width, social.height), (1280, 640));
        for theme in ["light", "dark"] {
            let wordmark = icon(&format!("brand/wordmark-{theme}.png"));
            assert_eq!((wordmark.width, wordmark.height), (1480, 336), "{theme}");
        }
        assert_eq!(committed(FAVICON), committed(&format!("{SOURCE_DIR}/favicon.svg")));
        let index = String::from_utf8(committed("ui/index.html")).unwrap();
        assert!(index.contains(r#"<link rel="icon" type="image/svg+xml" href="/favicon.svg" />"#));
    }

    #[test]
    fn the_bundle_config_names_the_generated_installer_art() {
        let conf: serde_json::Value =
            serde_json::from_slice(&committed("ui/src-tauri/tauri.conf.json")).unwrap();
        let bundle = &conf["bundle"];
        for (value, file) in [
            (&bundle["macOS"]["dmg"]["background"], "dmg-background.png"),
            (&bundle["windows"]["nsis"]["headerImage"], "nsis-header.bmp"),
            (&bundle["windows"]["nsis"]["sidebarImage"], "nsis-sidebar.bmp"),
            (&bundle["windows"]["wix"]["bannerPath"], "wix-banner.bmp"),
            (&bundle["windows"]["wix"]["dialogImagePath"], "wix-dialog.bmp"),
        ] {
            assert_eq!(value.as_str(), Some(format!("icons/installer/{file}").as_str()));
        }
    }

    #[test]
    fn a_bmp_is_bottom_up_bgr_padded_and_flattened_onto_white() {
        let image = Image { width: 1, height: 2, rgba: vec![255, 0, 0, 255, 0, 0, 0, 0] };
        let bmp = bmp(&image);
        assert_eq!(bmp.len(), 54 + 8);
        assert_eq!(
            &bmp[54..],
            [255, 255, 255, 0, 0, 0, 255, 0],
            "bottom row first, BGR, 4-byte rows"
        );
    }

    #[test]
    fn a_rect_view_box_is_read_and_a_missing_one_is_not() {
        assert_eq!(
            view_box(r#"<svg width="2" viewBox="0 0 493 58"><g/></svg>"#),
            Some([0, 0, 493, 58])
        );
        assert_eq!(view_box("<svg width=\"2\"><g viewBox=\"0 0 1 1\"/></svg>"), None);
    }

    #[test]
    fn icons_is_documented() {
        assert!(crate::USAGE.contains("cargo xtask icons "));
    }
}
