use stagecue_lib::core::persistence;
fn main() {
    match persistence::load(std::path::Path::new("examples/twilight-rehearsal.stagecue")) {
        Ok(p) => println!("OK: {} cues, {} assets, name={}", p.cues.len(), p.assets.len(), p.name),
        Err(e) => { println!("FAIL: {e}"); std::process::exit(1); }
    }
}
