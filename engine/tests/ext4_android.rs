//! Lecture d'un volume ext4 (Android) en lecture seule.
//!
//! Test activé seulement si `NF_EXT4_IMG` pointe vers une image ext4 brute
//! contenant `data/data/com.android.chrome/app_chrome/Default/History`
//! (construire avec `mkfs.ext4 -d <dossier> image.img`).

use navforensics_engine::e01::extract_browser_files;
use std::path::PathBuf;

#[test]
fn extrait_profils_android_depuis_ext4() {
    let Ok(img) = std::env::var("NF_EXT4_IMG") else {
        eprintln!("NF_EXT4_IMG absent : test ignoré");
        return;
    };
    let out = PathBuf::from(std::env::temp_dir()).join("nf_ext4_test");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();

    let files = extract_browser_files(&img, &out).expect("extraction ext4");

    let chrome = out.join("android/com.android.chrome/History");
    assert!(chrome.exists(), "History Chrome Android absent : {files:?}");
    assert!(files.iter().any(|p| p.ends_with("History")));

    let edge = out.join("android/com.microsoft.emmx/History");
    assert!(edge.exists(), "History Edge Android absent (chemin user/0) : {files:?}");

    let ff = out.join("android/org.mozilla.firefox/abc.default/places.sqlite");
    assert!(ff.exists(), "places.sqlite Firefox Android absent : {files:?}");
}
