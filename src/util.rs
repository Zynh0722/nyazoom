use rand::{
    distributions::{Alphanumeric, DistString},
    rngs::SmallRng,
    SeedableRng,
};

use std::{io, path::Path};

#[inline]
pub async fn make_dir<T>(name: T) -> io::Result<()>
where
    T: AsRef<Path>,
{
    tokio::fs::create_dir_all(name)
        .await
        .or_else(|err| match err.kind() {
            io::ErrorKind::AlreadyExists => Ok(()),
            _ => Err(err),
        })
}

#[inline]
pub fn get_random_name(len: usize) -> String {
    let mut rng = SmallRng::from_entropy();

    Alphanumeric.sample_string(&mut rng, len)
}

#[allow(dead_code)]
pub static UNITS: [&str; 6] = ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];

// This function is actually rather interesting to me, I understand that rust is
// very powerful, and its very safe, but i find it rather amusing that the [] operator
// doesn't check bounds, meaning it can panic at runtime. Usually rust is very
// very careful about possible panics
//
// although this function shouldn't be able to panic at runtime due to known bounds
// being listened to
#[inline]
pub fn _bytes_to_human_readable(bytes: u64) -> String {
    let mut running = bytes as f64;
    let mut count = 0;
    while running > 1024.0 && count <= 6 {
        running /= 1024.0;
        count += 1;
    }

    format!("{:.2} {}", running, UNITS[count - 1])
}
