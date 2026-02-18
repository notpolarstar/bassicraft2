use getrandom;

pub fn get_random_f32() -> Result<f32, getrandom::Error> {
    let mut buf = [0u8; 4];
    getrandom::fill(&mut buf)?;
    Ok(f32::from_ne_bytes(buf))
}

pub fn get_random_f32_normalized() -> Result<f32, getrandom::Error> {
    let mut buf = [0u8; 4];
    getrandom::fill(&mut buf)?;
    let value = u32::from_ne_bytes(buf);
    let normalized = value as f32 / u32::MAX as f32;
    Ok(2.0 * normalized - 1.0)
}
