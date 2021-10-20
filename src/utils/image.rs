use std::cmp::max;
use std::io::{BufReader, BufWriter};

use super::Result;

pub fn encode(src: &mut Vec<u8>) -> Result<Vec<u8>> {
    let base_len = src.len();
    let len = base_len + 4;
    let mut sq = (len as f32 / 3.0).sqrt() as usize;
    if sq * sq * 3 < len {
        sq += 1;
    }
    sq = max(10, sq);
    let mut padding: Vec<u8> = vec![
        ((base_len >> 24) & 0xFF) as u8,
        ((base_len >> 16) & 0xFF) as u8,
        ((base_len >> 8) & 0xFF) as u8,
        (base_len & 0xFF) as u8,
    ];
    padding.append(src);
    padding.resize(sq * sq * 3, 0);

    let mut ret = Vec::<u8>::new();
    let buf_writer = BufWriter::new(std::io::Cursor::new(&mut ret));
    let mut encoder = png::Encoder::new(buf_writer, sq as u32, sq as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&padding)?;
    std::mem::drop(writer);
    Ok(ret)
}

pub fn decode(src: &[u8]) -> Result<Vec<u8>> {
    let buf = std::io::Cursor::new(src);
    let decoder = png::Decoder::new(BufReader::new(buf));
    let mut reader = decoder.read_info()?;
    let mut ret = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut ret)?;
    let sz = ((ret[0] as u32) << 24)
        | ((ret[1] as u32) << 16)
        | ((ret[2] as u32) << 8)
        | (ret[3] as u32);
    let slice = &ret[4..4 + sz as usize];
    Ok(slice.to_vec())
}
