//! Déchiffrement des identifiants enregistrés par Firefox (et dérivés Gecko).
//!
//! Équivalent Rust de l'outil forensique public `firefox_decrypt` (Unode) :
//! dérive la clé maîtresse NSS depuis `key4.db`, puis déchiffre les champs
//! `encryptedUsername` / `encryptedPassword` de `logins.json`.
//!
//! Deux schémas de stockage de la clé maîtresse sont pris en charge :
//!   - 3DES-CBC (pbeWithSha1AndTripleDES-CBC), Firefox ancien ;
//!   - AES-256-CBC via PBES2/PBKDF2-HMAC-SHA256, Firefox ≥ 75.
//! Les entrées de `logins.json` sont toujours chiffrées en 3DES-CBC avec la
//! clé maîtresse. Le mot de passe maître de l'utilisateur est pris en charge
//! (chaîne vide par défaut).
//!
//! Tout est fait en lecture seule sur une copie (via `ReadOnlyDb`). Aucune
//! donnée n'est écrite dans le profil d'origine.

use crate::db::ReadOnlyDb;
use base64::Engine;
use cipher::block_padding::NoPadding;
use cipher::{BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::path::Path;

type Dec3 = cbc::Decryptor<des::TdesEde3>;
type DecAes = cbc::Decryptor<aes::Aes256>;
type HmacSha1 = Hmac<Sha1>;

/// Identifiant déchiffré.
#[derive(Debug, Clone)]
pub struct FxLogin {
    pub url: String,
    pub username: String,
    pub password: String,
}

/// CKA_ID de la clé maîtresse 3DES dans `nssPrivate`.
const CKA_ID: [u8; 16] = [
    0xf8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
];

// OID (contenu DER, sans le tag/longueur).
const OID_3DES_PBE: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x0c, 0x05, 0x01, 0x03];
const OID_PBES2: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x05, 0x0d];
/// AES-256-CBC (2.16.840.1.101.3.4.1.42) : chiffrement des champs logins récents.
const OID_AES256_CBC: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x01, 0x2a];

// --- Lecteur DER minimal (tag, longueur, valeur) ---------------------------

struct Tlv<'a> {
    tag: u8,
    content: &'a [u8],
}

/// Lit un seul TLV ; renvoie (élément, reste).
fn read_tlv(b: &[u8]) -> Result<(Tlv<'_>, &[u8]), String> {
    if b.len() < 2 {
        return Err("DER trop court".into());
    }
    let first = b[1];
    let mut i = 2;
    let len = if first & 0x80 == 0 {
        first as usize
    } else {
        let n = (first & 0x7f) as usize;
        if n == 0 || n > 4 || b.len() < i + n {
            return Err("DER : longueur invalide".into());
        }
        let mut l = 0usize;
        for _ in 0..n {
            l = (l << 8) | b[i] as usize;
            i += 1;
        }
        l
    };
    if b.len() < i + len {
        return Err("DER tronqué".into());
    }
    Ok((Tlv { tag: b[0], content: &b[i..i + len] }, &b[i + len..]))
}

/// Découpe le contenu d'une SEQUENCE en ses éléments TLV successifs.
fn seq_items(content: &[u8]) -> Result<Vec<Tlv<'_>>, String> {
    let mut out = Vec::new();
    let mut rest = content;
    while !rest.is_empty() {
        let (tlv, r) = read_tlv(rest)?;
        out.push(tlv);
        rest = r;
    }
    Ok(out)
}

/// Lit un entier DER (gros-boutiste) en usize.
fn der_int(content: &[u8]) -> usize {
    let mut v = 0usize;
    for &b in content {
        v = (v << 8) | b as usize;
    }
    v
}

// --- Primitives crypto -----------------------------------------------------

fn sha1_of(parts: &[&[u8]]) -> [u8; 20] {
    let mut h = Sha1::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
    let mut m = HmacSha1::new_from_slice(key).expect("HMAC accepte toute taille de clé");
    m.update(data);
    m.finalize().into_bytes().into()
}

/// Dérivation NSS pour pbeWithSha1AndTripleDES-CBC : renvoie (clé 24, iv 8).
fn derive_3des(global_salt: &[u8], master_pw: &[u8], entry_salt: &[u8]) -> ([u8; 24], [u8; 8]) {
    let hp = sha1_of(&[global_salt, master_pw]);
    let mut pes = entry_salt.to_vec();
    if pes.len() < 20 {
        pes.resize(20, 0);
    } else {
        pes.truncate(20);
    }
    let chp = sha1_of(&[&hp, entry_salt]);
    let mut d1 = pes.clone();
    d1.extend_from_slice(entry_salt);
    let k1 = hmac_sha1(&chp, &d1);
    let tk = hmac_sha1(&chp, &pes);
    let mut d2 = tk.to_vec();
    d2.extend_from_slice(entry_salt);
    let k2 = hmac_sha1(&chp, &d2);
    let mut k = [0u8; 40];
    k[..20].copy_from_slice(&k1);
    k[20..].copy_from_slice(&k2);
    let mut key = [0u8; 24];
    key.copy_from_slice(&k[..24]);
    let mut iv = [0u8; 8];
    iv.copy_from_slice(&k[32..40]);
    (key, iv)
}

fn cbc_3des_decrypt(key: &[u8], iv: &[u8], ct: &[u8]) -> Result<Vec<u8>, String> {
    if ct.is_empty() || ct.len() % 8 != 0 {
        return Err("3DES : longueur de bloc invalide".into());
    }
    Dec3::new_from_slices(key, iv)
        .map_err(|_| "3DES : clé/iv invalide".to_string())?
        .decrypt_padded_vec_mut::<NoPadding>(ct)
        .map_err(|_| "3DES : échec du déchiffrement".to_string())
}

fn cbc_aes256_decrypt(key: &[u8], iv: &[u8], ct: &[u8]) -> Result<Vec<u8>, String> {
    if ct.is_empty() || ct.len() % 16 != 0 {
        return Err("AES : longueur de bloc invalide".into());
    }
    DecAes::new_from_slices(key, iv)
        .map_err(|_| "AES : clé/iv invalide".to_string())?
        .decrypt_padded_vec_mut::<NoPadding>(ct)
        .map_err(|_| "AES : échec du déchiffrement".to_string())
}

/// Retire un rembourrage PKCS#7 si présent et plausible.
fn strip_pkcs7(mut v: Vec<u8>, block: usize) -> Vec<u8> {
    if let Some(&n) = v.last() {
        let n = n as usize;
        if n >= 1 && n <= block && n <= v.len() && v[v.len() - n..].iter().all(|&b| b as usize == n) {
            v.truncate(v.len() - n);
        }
    }
    v
}

// --- Déchiffrement d'une entrée PBE (item2 / a11) ---------------------------

/// Déchiffre une entrée protégée (SEQUENCE{ algId, OCTETSTRING ct }) selon son
/// OID : 3DES-CBC ou AES-256-CBC (PBES2).
fn decrypt_pbe_entry(global_salt: &[u8], master_pw: &[u8], der: &[u8]) -> Result<Vec<u8>, String> {
    let (top, _) = read_tlv(der)?;
    let items = seq_items(top.content)?;
    if items.len() < 2 {
        return Err("entrée PBE : structure inattendue".into());
    }
    let algid = seq_items(items[0].content)?;
    if algid.len() < 2 {
        return Err("entrée PBE : algorithme absent".into());
    }
    let oid = algid[0].content;
    let ct = items[1].content;

    if oid == OID_3DES_PBE {
        // params = SEQUENCE { OCTETSTRING entrySalt, INTEGER iter }
        let params = seq_items(algid[1].content)?;
        let entry_salt = params
            .first()
            .ok_or("3DES : sel absent")?
            .content;
        let (key, iv) = derive_3des(global_salt, master_pw, entry_salt);
        cbc_3des_decrypt(&key, &iv, ct)
    } else if oid == OID_PBES2 {
        // params = SEQUENCE { kdf=SEQUENCE{OID, SEQUENCE{salt,iter,keylen,prf}},
        //                     enc=SEQUENCE{OID aes, OCTETSTRING iv14} }
        let params = seq_items(algid[1].content)?;
        if params.len() < 2 {
            return Err("PBES2 : paramètres incomplets".into());
        }
        let kdf = seq_items(params[0].content)?;
        let kdf_params = seq_items(kdf.get(1).ok_or("PBES2 : KDF absente")?.content)?;
        let entry_salt = kdf_params.first().ok_or("PBES2 : sel absent")?.content;
        let iter = der_int(kdf_params.get(1).ok_or("PBES2 : itérations absentes")?.content);
        let key_len = kdf_params.get(2).map(|t| der_int(t.content)).unwrap_or(32);
        if key_len != 32 {
            return Err("PBES2 : longueur de clé non gérée".into());
        }
        let enc = seq_items(params[1].content)?;
        let iv14 = enc.get(1).ok_or("PBES2 : iv absent")?.content;

        let k = sha1_of(&[global_salt, master_pw]);
        let mut key = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(&k, entry_salt, iter as u32, &mut key);
        // NSS stocke un iv de 14 octets ; l'iv AES réel est 0x04 0x0e + ces 14.
        let mut iv = Vec::with_capacity(16);
        iv.extend_from_slice(&[0x04, 0x0e]);
        iv.extend_from_slice(iv14);
        if iv.len() != 16 {
            return Err("PBES2 : iv de taille inattendue".into());
        }
        cbc_aes256_decrypt(&key, &iv, ct)
    } else {
        Err("schéma de chiffrement NSS non géré".into())
    }
}

// --- Extraction de la clé maîtresse depuis key4.db -------------------------

/// Candidats de clé dans le contenu déchiffré de a11 : le buffer entier s'il
/// fait 24 ou 32 octets, puis tout OCTET STRING de 24 ou 32 octets trouvé dans
/// une structure DER (parcours récursif des SEQUENCE). Le bon candidat est
/// choisi plus tard, en vérifiant qu'il déchiffre réellement un identifiant.
fn collect_key_candidates(buf: &[u8], out: &mut Vec<Vec<u8>>, depth: u8) {
    if (buf.len() == 24 || buf.len() == 32) && !out.iter().any(|k| k == buf) {
        out.push(buf.to_vec());
    }
    if depth > 4 {
        return;
    }
    let mut rest = buf;
    while rest.len() >= 2 {
        let Ok((tlv, next)) = read_tlv(rest) else { break };
        if tlv.tag == 0x04 && (tlv.content.len() == 24 || tlv.content.len() == 32) {
            if !out.iter().any(|k| k == tlv.content) {
                out.push(tlv.content.to_vec());
            }
        } else if tlv.tag == 0x30 {
            collect_key_candidates(tlv.content, out, depth + 1);
        }
        rest = next;
    }
}

/// Squelette DER (tags et longueurs uniquement, sans valeurs) pour diagnostic.
fn der_skeleton(buf: &[u8]) -> String {
    let mut out = Vec::new();
    let mut rest = buf;
    while rest.len() >= 2 && out.len() < 12 {
        let Ok((tlv, next)) = read_tlv(rest) else { break };
        let consumed = rest.len() - next.len();
        let hdr = consumed - tlv.content.len();
        out.push(format!("tag {:02x} len {}", tlv.tag, tlv.content.len()));
        if tlv.tag == 0x30 {
            out.push(format!("[{}]", der_skeleton(tlv.content)));
        }
        let _ = hdr;
        rest = next;
    }
    out.join(", ")
}

fn master_key(db: &ReadOnlyDb, master_pw: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let (global_salt, item2): (Vec<u8>, Vec<u8>) = db
        .conn
        .query_row(
            "SELECT item1, item2 FROM metaData WHERE id = 'password'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("key4.db (metaData) illisible : {e}"))?;

    // Vérifie le mot de passe maître via l'entrée de contrôle.
    let check = decrypt_pbe_entry(&global_salt, master_pw, &item2)?;
    if !check.starts_with(b"password-check") {
        return Err("Mot de passe maître incorrect (ou base protégée).".into());
    }

    // Clé maîtresse 3DES : entrée a11 de nssPrivate (a102 == CKA_ID).
    let mut a11: Option<Vec<u8>> = None;
    {
        let mut stmt = db
            .conn
            .prepare("SELECT a11, a102 FROM nssPrivate")
            .map_err(|e| format!("key4.db (nssPrivate) : {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                let v: Option<Vec<u8>> = r.get(0)?;
                let id: Option<Vec<u8>> = r.get(1)?;
                Ok((v, id))
            })
            .map_err(|e| e.to_string())?;
        let mut first: Option<Vec<u8>> = None;
        for row in rows.flatten() {
            let (v, id) = row;
            if let Some(v) = v {
                if first.is_none() {
                    first = Some(v.clone());
                }
                if id.as_deref() == Some(&CKA_ID[..]) {
                    a11 = Some(v);
                    break;
                }
            }
        }
        if a11.is_none() {
            a11 = first;
        }
    }
    let a11 = a11.ok_or("Clé maîtresse absente de key4.db (nssPrivate).")?;
    let decrypted = decrypt_pbe_entry(&global_salt, master_pw, &a11)?;
    let mut cands = Vec::new();
    collect_key_candidates(&decrypted, &mut cands, 0);
    // Structure inconnue : toute fenêtre de 24 ou 32 octets, à chaque décalage.
    // Le choix final se fait par déchiffrement d'un identifiant réel.
    for len in [24usize, 32] {
        if decrypted.len() >= len {
            for off in 0..=decrypted.len() - len {
                let w = &decrypted[off..off + len];
                if !cands.iter().any(|k| k == w) {
                    cands.push(w.to_vec());
                }
            }
        }
    }
    if cands.is_empty() {
        return Err(format!(
            "Clé maîtresse non reconnue ({} octets déchiffrés, en-têtes : {}).",
            decrypted.len(),
            der_skeleton(&decrypted)
        ));
    }
    Ok(cands)
}

// --- Déchiffrement d'un champ de logins.json --------------------------------

/// Déchiffre un champ base64 (SEQUENCE{ keyId, SEQUENCE{OID des3, iv}, ct }).
fn decrypt_login_field(master_key: &[u8], b64: &str) -> Result<String, String> {
    let der = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|_| "champ base64 invalide".to_string())?;
    let (top, _) = read_tlv(&der)?;
    let items = seq_items(top.content)?;
    if items.len() < 3 {
        return Err("entrée login : structure inattendue".into());
    }
    let algo = seq_items(items[1].content)?;
    let oid = algo.first().ok_or("entrée login : OID absent")?.content;
    let iv = algo.get(1).ok_or("entrée login : iv absent")?.content;
    let ct = items[2].content;
    // L'OID choisit l'algorithme : AES-256-CBC (profils récents) ou 3DES-CBC.
    let pt = if oid == OID_AES256_CBC {
        strip_pkcs7(cbc_aes256_decrypt(master_key, iv, ct)?, 16)
    } else {
        strip_pkcs7(cbc_3des_decrypt(master_key, iv, ct)?, 8)
    };
    String::from_utf8(pt).map_err(|_| "texte non UTF-8 (mauvaise clé)".to_string())
}

// --- API publique -----------------------------------------------------------

/// Déchiffre tous les identifiants d'un profil Firefox (dossier contenant
/// `key4.db` et `logins.json`). `master_pw` peut être vide.
pub fn decrypt_profile(profile_dir: &Path, master_pw: &str) -> Result<Vec<FxLogin>, String> {
    let key4 = profile_dir.join("key4.db");
    let logins = profile_dir.join("logins.json");
    if !key4.exists() {
        return Err(format!("key4.db introuvable dans {}", profile_dir.display()));
    }
    if !logins.exists() {
        return Err(format!("logins.json introuvable dans {}", profile_dir.display()));
    }
    let db = ReadOnlyDb::open(&key4)?;
    let cands = master_key(&db, master_pw.as_bytes())?;

    let text = std::fs::read_to_string(&logins).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let arr = json
        .get("logins")
        .and_then(|v| v.as_array())
        .ok_or("logins.json : clé « logins » absente")?;

    // Choix de la clé : le premier candidat qui déchiffre un identifiant réel.
    let sample = arr.iter().find_map(|it| {
        let u = it.get("encryptedUsername").and_then(|v| v.as_str())?;
        let p = it.get("encryptedPassword").and_then(|v| v.as_str())?;
        Some((u.to_string(), p.to_string()))
    });
    let mk: Vec<u8> = match &sample {
        None => cands[0].clone(),
        Some((u, p)) => cands
            .iter()
            .find(|k| decrypt_login_field(k, u).is_ok() || decrypt_login_field(k, p).is_ok())
            .cloned()
            .ok_or("Clé maîtresse : aucun candidat ne déchiffre les identifiants.")?,
    };

    let mut out = Vec::new();
    for item in arr {
        let url = item
            .get("origin")
            .or_else(|| item.get("hostname"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let enc_u = item.get("encryptedUsername").and_then(|v| v.as_str());
        let enc_p = item.get("encryptedPassword").and_then(|v| v.as_str());
        let (Some(eu), Some(ep)) = (enc_u, enc_p) else { continue };
        let username = decrypt_login_field(&mk, eu).unwrap_or_else(|e| format!("<erreur : {e}>"));
        let password = decrypt_login_field(&mk, ep).unwrap_or_else(|e| format!("<erreur : {e}>"));
        out.push(FxLogin { url, username, password });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_parse_sequence() {
        // SEQUENCE { OCTETSTRING "AB", INTEGER 0x1234 }
        let der = [0x30, 0x08, 0x04, 0x02, b'A', b'B', 0x02, 0x02, 0x12, 0x34];
        let (top, _) = read_tlv(&der).unwrap();
        assert_eq!(top.content.len(), 8);
        let items = seq_items(top.content).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].content, b"AB");
        assert_eq!(der_int(items[1].content), 0x1234);
    }

    #[test]
    fn der_long_form_length() {
        // OCTETSTRING de 200 octets (forme longue de longueur : 0x81 0xC8).
        let mut der = vec![0x04, 0x81, 0xC8];
        der.extend(std::iter::repeat(0xAB).take(200));
        let (tlv, rest) = read_tlv(&der).unwrap();
        assert_eq!(tlv.content.len(), 200);
        assert!(rest.is_empty());
    }

    #[test]
    fn tdes_cbc_known_vector() {
        // NIST-style : clé 24 octets, iv 8, un bloc. On vérifie l'aller-retour
        // en re-chiffrant via le même mode pour confirmer l'implémentation CBC.
        use cipher::BlockEncryptMut;
        type Enc3 = cbc::Encryptor<des::TdesEde3>;
        let key = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd,
            0xef, 0x01, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23,
        ];
        let iv = [0u8; 8];
        let pt = *b"8bytes!!";
        let ct = Enc3::new_from_slices(&key, &iv)
            .unwrap()
            .encrypt_padded_vec_mut::<NoPadding>(&pt);
        let back = cbc_3des_decrypt(&key, &iv, &ct).unwrap();
        assert_eq!(&back, &pt);
    }

    #[test]
    fn aes256_cbc_roundtrip() {
        use cipher::BlockEncryptMut;
        type EncAes = cbc::Encryptor<aes::Aes256>;
        let key = [7u8; 32];
        let iv = [9u8; 16];
        let pt = *b"16-byte-block!!!";
        let ct = EncAes::new_from_slices(&key, &iv)
            .unwrap()
            .encrypt_padded_vec_mut::<NoPadding>(&pt);
        let back = cbc_aes256_decrypt(&key, &iv, &ct).unwrap();
        assert_eq!(&back, &pt);
    }

    #[test]
    fn login_field_aes256() {
        use cipher::BlockEncryptMut;
        type EncAes = cbc::Encryptor<aes::Aes256>;
        let key = [5u8; 32];
        let iv = [3u8; 16];
        // Texte clair « secret » + rembourrage PKCS#7 sur un bloc de 16.
        let mut pt = b"secret".to_vec();
        pt.extend(std::iter::repeat(10u8).take(10));
        let ct = EncAes::new_from_slices(&key, &iv)
            .unwrap()
            .encrypt_padded_vec_mut::<NoPadding>(&pt);
        // DER : SEQUENCE { OCTET keyId, SEQUENCE { OID aes256, OCTET iv }, OCTET ct }
        let mut algo = vec![0x06, OID_AES256_CBC.len() as u8];
        algo.extend_from_slice(OID_AES256_CBC);
        algo.extend([0x04, iv.len() as u8]);
        algo.extend_from_slice(&iv);
        let mut algo_seq = vec![0x30, algo.len() as u8];
        algo_seq.extend(algo);
        let mut body = vec![0x04, 0x02, 0x01, 0x02];
        body.extend(algo_seq);
        body.extend([0x04, ct.len() as u8]);
        body.extend(&ct);
        let mut der = vec![0x30, body.len() as u8];
        der.extend(body);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&der);
        assert_eq!(decrypt_login_field(&key, &b64).unwrap(), "secret");
    }

    #[test]
    fn key_candidates_from_der() {
        // SEQUENCE { SEQUENCE { OID 1.2 }, OCTET STRING (32 octets) } = 48 octets.
        let mut inner = vec![0x30, 0x03, 0x06, 0x01, 0x2a];
        inner.extend([0x04, 0x20]);
        inner.extend([0xabu8; 32]);
        let mut der = vec![0x30, inner.len() as u8];
        der.extend(&inner);
        let mut cands = Vec::new();
        collect_key_candidates(&der, &mut cands, 0);
        assert_eq!(cands, vec![vec![0xabu8; 32]]);

        // Un buffer brut de 48 octets ne produit aucun candidat.
        let mut none = Vec::new();
        collect_key_candidates(&[0x11u8; 48], &mut none, 0);
        assert!(none.is_empty());
    }

    #[test]
    fn pbkdf2_sha256_rfc_like() {
        // P="password", S="salt", c=1, dkLen=32 (vecteur de référence connu).
        let mut out = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(b"password", b"salt", 1, &mut out);
        assert_eq!(
            hex::encode(out),
            "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
        );
    }

    #[test]
    fn pkcs7_strip_ok() {
        let v = vec![1u8, 2, 3, 5, 5, 5, 5, 5];
        assert_eq!(strip_pkcs7(v, 8), vec![1, 2, 3]);
        // pas de rembourrage plausible -> inchangé
        let v2 = vec![1u8, 2, 3, 4, 5, 6, 7, 9];
        assert_eq!(strip_pkcs7(v2.clone(), 8), v2);
    }
}
