//! Mutated session files load (or fail) cleanly and render without panicking.

use proptest::prelude::*;
use serde_json::Value;

/// Collect JSON pointers to every number in `v`.
fn number_paths(v: &Value, path: String, out: &mut Vec<String>) {
    match v {
        Value::Number(_) => out.push(path),
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                number_paths(x, format!("{path}/{i}"), out);
            }
        }
        Value::Object(o) => {
            for (k, x) in o {
                number_paths(x, format!("{path}/{}", k.replace('~', "~0").replace('/', "~1")), out);
            }
        }
        _ => {}
    }
}

fn demo_json() -> Value {
    let e = soundcraft_engine::demo::demo_engine();
    serde_json::to_value(e.session()).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, .. ProptestConfig::default() })]
    #[test]
    fn mutated_sessions_never_panic(picks in proptest::collection::vec((any::<u32>(), prop_oneof![
        Just(-1.0e18f64), Just(-1.0), Just(0.0), Just(1.0e18), Just(4.0e9), Just(-0.5), Just(1.0e-300), Just(123_456_789.0)
    ]), 1..12)) {
        let demo = soundcraft_engine::demo::demo_engine();
        let mut v = demo_json();
        let mut paths = Vec::new();
        number_paths(&v, String::new(), &mut paths);
        prop_assume!(!paths.is_empty());
        for (k, val) in picks {
            let p = &paths[k as usize % paths.len()];
            if let Some(slot) = v.pointer_mut(p) {
                // Keep the JSON number kind so the file still parses and reaches the mixer.
                *slot = if slot.is_u64() {
                    serde_json::json!(if val < 0.0 { 0u64 } else { (val.min(9.0e15)) as u64 })
                } else if slot.is_i64() {
                    serde_json::json!(val.clamp(-9.0e15, 9.0e15) as i64)
                } else {
                    serde_json::json!(val)
                };
            }
        }
        let text = v.to_string();
        let parsed = soundcraft_model::Session::from_json(&text);
        if let Ok(mut s) = parsed {
            // Re-attach the decoded audio as the app would after loading the files.
            s.pool = demo.session().pool.clone();
            let start = s.edit.selection.start.clamp(0, 1 << 40);
            let r = soundcraft_time::Range::new(start, start + 2048);
            let out = soundcraft_mix::render_range(&s, r, 512);
            prop_assert!(out.iter().all(|c| c.iter().all(|x| x.is_finite())));
            let mut e = soundcraft_engine::Engine::new(s);
            let _ = e.execute("session.inspect", &serde_json::json!({}));
            let _ = e.execute("edit.select_all", &serde_json::json!({}));
            let _ = e.execute("edit.duplicate", &serde_json::json!({}));
        }
    }
}
