use navforensics_engine as nf;
use std::io::Write;

#[test]
fn carves_images_and_videos() {
    let dir = std::env::temp_dir().join("nf_media_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();

    // JPEG factice
    let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE0];
    jpg.extend_from_slice(&[0u8; 100]);
    std::fs::File::create(dir.join("f_000001")).unwrap().write_all(&jpg).unwrap();

    // PNG factice
    let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    png.extend_from_slice(&[0u8; 50]);
    std::fs::File::create(dir.join("sub/f_000002")).unwrap().write_all(&png).unwrap();

    // MP4 factice (ftyp a l'offset 4)
    let mut mp4 = vec![0x00, 0x00, 0x00, 0x18];
    mp4.extend_from_slice(b"ftypmp42");
    mp4.extend_from_slice(&[0u8; 80]);
    std::fs::File::create(dir.join("vid.bin")).unwrap().write_all(&mp4).unwrap();

    // fichier non-media
    std::fs::File::create(dir.join("texte.txt")).unwrap().write_all(b"rien a voir").unwrap();

    let items = nf::carve_cache_media(&dir, nf::MEDIA_MAX_ITEMS, nf::MEDIA_MAX_BYTES);
    assert_eq!(items.len(), 3, "3 medias (txt ignore)");
    let mimes: Vec<&str> = items.iter().map(|m| m.mime.as_str()).collect();
    assert!(mimes.contains(&"image/jpeg"));
    assert!(mimes.contains(&"image/png"));
    assert!(mimes.contains(&"video/mp4"));

    // round-trip base64 : le JPEG decode doit redonner les octets d'origine
    let j = items.iter().find(|m| m.mime == "image/jpeg").unwrap();
    let dec = nf::b64_decode(&j.data);
    assert_eq!(&dec[0..4], &[0xFF, 0xD8, 0xFF, 0xE0]);
    assert_eq!(dec.len(), j.size);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn carves_html_pages_incl_gzip() {
    use std::io::Write as _;
    let dir = std::env::temp_dir().join("nf_html_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let html = r#"<!DOCTYPE html><html><head><title>Page Test</title>
<link rel="stylesheet" href="/css/site.css">
<script src="https://cdn.example.com/app.js"></script></head>
<body><img src="/img/photo.png"><style>.x{background:url('bg.webp')}</style></body></html>"#;

    // page HTML en clair
    std::fs::File::create(dir.join("f_aaa")).unwrap().write_all(html.as_bytes()).unwrap();

    // meme page compressee gzip (comme souvent en cache)
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(html.as_bytes()).unwrap();
    let gz = enc.finish().unwrap();
    std::fs::File::create(dir.join("f_bbb")).unwrap().write_all(&gz).unwrap();

    let items = nf::carve_cache_html(&dir, nf::HTML_MAX_ITEMS);
    // les deux pages sont identiques apres decompression -> dedupliquees -> 1
    assert_eq!(items.len(), 1, "page dedupliquee");
    let it = &items[0];
    assert_eq!(it.title, "Page Test");
    let urls: Vec<&str> = it.resources.iter().map(|r| r.url.as_str()).collect();
    assert!(urls.contains(&"/css/site.css"));
    assert!(urls.contains(&"https://cdn.example.com/app.js"));
    assert!(urls.contains(&"/img/photo.png"));
    assert!(urls.contains(&"bg.webp"));
    let kinds: std::collections::HashSet<&str> = it.resources.iter().map(|r| r.kind.as_str()).collect();
    assert!(kinds.contains("script") && kinds.contains("style") && kinds.contains("image"));
    assert!(it.source.contains("<title>Page Test</title>"), "code source present");

    let _ = std::fs::remove_dir_all(&dir);
}
