use std::collections::BTreeMap;

fn main() {
    let input = {
        let stdin = std::io::stdin();
        stdin.lines()
    };

    let mut map = BTreeMap::new();

    for i in input.map(Result::unwrap) {
        let (addr, rest) = i.split_once(" ").unwrap();
        let (ty, name) = rest.split_once(" ").unwrap();
        let addr = u64::from_str_radix(addr, 16).unwrap();

        if ty == "t" || ty == "T" { map.insert(addr, name.to_owned()); }
    }

    for (k, v) in map {
        println!("{k:x} {v}")
    }
}
