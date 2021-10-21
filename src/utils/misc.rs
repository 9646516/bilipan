use super::Result;
use aes::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, NewBlockCipher};
use std::path::Path;

use std::io::Write;

const CHR: &str = "0123456789abcdef";

pub fn get_input(sb: &mut String) -> Result<()> {
    sb.clear();
    loop {
        std::io::stdin().read_line(sb)?;
        *sb = sb.trim().to_owned();
        if !sb.is_empty() {
            break;
        }
    }
    Ok(())
}

pub fn combine(aes_key: Vec<u8>, idx_url: String) -> Result<String> {
    let url = regex::Regex::new("i0.hdslb.com/bfs/album/(.*?).png")?
        .captures_iter(&idx_url)
        .next()
        .ok_or("")?
        .get(1)
        .ok_or("")?
        .as_str()
        .to_string();
    let mut ret = String::with_capacity(40 + aes_key.len());
    for i in &aes_key {
        let hi = ((*i) >> 4) & 0xF;
        let lo = (*i) & 0xF;
        ret.push(CHR.chars().nth(hi as usize).unwrap());
        ret.push(CHR.chars().nth(lo as usize).unwrap());
    }
    ret.push_str(&url);
    Ok(ret)
}

pub fn split(src: &str) -> Result<(Vec<u8>, String)> {
    let (a, b) = src.split_at(64);
    let mut fst = Vec::new();
    let snd = b.to_owned();
    for i in 0..32 {
        let hi = u8::from_str_radix(&a[(i + i)..(i + i + 1)], 16)?;
        let lo = u8::from_str_radix(&a[(i + i + 1)..(i + i + 1 + 1)], 16)?;
        fst.push(hi << 4 | lo);
    }
    Ok((fst, snd))
}

pub fn generate_idx(
    src_path: &Path,
    xor_key: u8,
    aes_key: &[u8],
    src: &[String],
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut stringio = std::io::Cursor::new(&mut buf);
    let mut data = json::JsonValue::new_array();
    for i in src {
        data.push(i.as_str())?;
    }
    let sz = src_path.metadata()?.len();
    let filename = src_path.file_name().ok_or("")?.to_str().ok_or("")?;
    let dp = json::object! {
        xor_key: xor_key,
        list: data,
        name:filename,
        sz:sz
    };
    dp.write(&mut stringio)?;
    let key = GenericArray::from_slice(aes_key);

    let cipher = aes::Aes256::new(key);
    let chunk_size = 128 / 8;
    buf.resize(
        (buf.len() / chunk_size + (if buf.len() % chunk_size == 0 { 0 } else { 1 })) * chunk_size,
        0,
    );
    let mut buf2 = Vec::new();
    let mut stringio2 = std::io::Cursor::new(&mut buf2);
    for x in buf.chunks(chunk_size) {
        let mut ss = GenericArray::clone_from_slice(x);
        cipher.encrypt_block(&mut ss);
        stringio2.write_all(ss.as_slice())?;
    }
    let res = super::encode(&mut buf2)?;
    Ok(res)
}

pub fn decrypt_aes_single(src: Vec<u8>, aes_key: &[u8]) -> Result<(u8, String, u64, Vec<String>)> {
    let buf = super::decode(&src)?;
    let key = GenericArray::from_slice(aes_key);
    let cipher = aes::Aes256::new(key);
    let mut buf2 = Vec::new();
    for x in buf.chunks(128 / 8) {
        let mut ss = GenericArray::clone_from_slice(x);
        cipher.decrypt_block(&mut ss);
        buf2.extend_from_slice(ss.as_slice());
    }
    let mut s = String::from_utf8(buf2)?;
    while !s.is_empty() && s.chars().nth_back(0).ok_or("")? as u8 == 0 {
        s.pop();
    }
    let dp = json::parse(&s)?;
    let xor_key = (&dp["xor_key"]).as_u8().ok_or("")?;
    let list = &dp["list"];
    let mut ret = Vec::new();
    for i in 0..list.len() {
        let sx = list[i].as_str().ok_or("")?;
        ret.push(sx.to_owned());
    }
    let filename = (&dp["name"]).as_str().ok_or("")?.to_string();
    let sz = (&dp["sz"]).as_u64().ok_or("")?;

    Ok((xor_key, filename, sz, ret))
}
