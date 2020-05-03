use std::path::Path;
use std::fs::File;

/// Return the highest number that still has a corresponding msgpack file
/// in the directory `/state`.
pub fn latest_idx() -> Option<usize> {
    let mut n = 0usize;
    while Path::new(&format!("{}.msgpack", n)).exists() {
        n += 1;
    }
    n.checked_sub(1)
}

/// Read state from the file `{n}.msgpack`.
pub fn load(n: usize) -> rmpv::Value {
    let mut file = File::open(format!("{}.msgpack", n)).unwrap();
    rmpv::decode::value::read_value(&mut file).unwrap()
}

/// Generate a new state.
pub fn new() -> rmpv::Value {
    use rmpv::Value::Array;
    // { people: [[0, 0], [10, 10]] }
    vec![("people".into(), Array(vec![
        Array(vec![0.into(), 0.into()]),
        Array(vec![10.into(), 10.into()]),
    ]))].into()
}