use std::collections::BTreeMap;

use craft_plugin::{MAX_DATA_KEY, MAX_DATA_KEYS, MAX_DATA_PER_PLUGIN, MAX_DATA_VALUE};

pub type Data = BTreeMap<String, Vec<u8>>;

pub fn size(data: &Data) -> usize {
    data.iter().map(|(key, value)| key.len() + value.len()).sum()
}

pub fn write(place: &mut Data, elsewhere: usize, key: &str, value: Option<Vec<u8>>) -> Result<(), String> {
    let Some(value) = value else {
        place.remove(key);
        return Ok(());
    };
    if key.is_empty() || key.len() > MAX_DATA_KEY {
        return Err(format!("a data key is 1 to {MAX_DATA_KEY} bytes long, not {}", key.len()));
    }
    if value.len() > MAX_DATA_VALUE {
        return Err(format!(
            "`{key}` would hold {} bytes, and one data value holds at most {} KiB; split it over several keys or keep it in a file under Root::Data",
            value.len(),
            MAX_DATA_VALUE >> 10
        ));
    }
    let old = place.get(key).map(|old| key.len() + old.len());
    if old.is_none() && place.len() >= MAX_DATA_KEYS {
        return Err(format!(
            "this place already holds {MAX_DATA_KEYS} keys of this plugin's data; remove one with host::remove_data before adding `{key}`"
        ));
    }
    let total = elsewhere + size(place) - old.unwrap_or(0) + key.len() + value.len();
    if total > MAX_DATA_PER_PLUGIN {
        return Err(format!(
            "writing `{key}` would bring this plugin's data in the project to {total} bytes, and a plugin keeps at most {} MiB in one project; remove data it no longer needs",
            MAX_DATA_PER_PLUGIN >> 20
        ));
    }
    place.insert(key.to_string(), value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_within_the_limits_and_refuses_past_them() {
        let mut place = Data::new();
        write(&mut place, 0, "track", Some(vec![1, 2, 3])).unwrap();
        assert_eq!(place.get("track"), Some(&vec![1, 2, 3]));
        assert_eq!(size(&place), 8);
        let big = write(&mut place, 0, "big", Some(vec![0; MAX_DATA_VALUE + 1])).unwrap_err();
        assert!(big.contains("at most 256 KiB"), "{big}");
        let full = write(&mut place, MAX_DATA_PER_PLUGIN - 8, "more", Some(vec![0])).unwrap_err();
        assert!(full.contains("at most 4 MiB"), "{full}");
        write(&mut place, MAX_DATA_PER_PLUGIN - 8, "track", Some(vec![9, 9, 9])).unwrap();
        for n in 1..MAX_DATA_KEYS {
            write(&mut place, 0, &format!("k{n}"), Some(Vec::new())).unwrap();
        }
        let crowded = write(&mut place, 0, "one-more", Some(Vec::new())).unwrap_err();
        assert!(crowded.contains("64 keys"), "{crowded}");
        write(&mut place, 0, "track", None).unwrap();
        assert_eq!(place.len(), MAX_DATA_KEYS - 1);
        assert!(write(&mut place, 0, "", Some(Vec::new())).unwrap_err().contains("1 to 256 bytes"));
    }
}
