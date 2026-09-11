//! Regression for the actual provider -> JSON value -> persisted JSON path.
use serde_json::Value;

#[test]
fn provider_decimal_timestep_preserves_the_requested_float_bits() {
    let arguments = r#"{"parameters":{"timestep":0.0015707963267948967}}"#;
    let parsed: Value = serde_json::from_str(arguments).unwrap();
    let expected = 0.0015707963267948967_f64;
    assert_eq!(parsed["parameters"]["timestep"].as_f64().unwrap().to_bits(), expected.to_bits(),
        "A correctly supplied provider decimal must not change before solver admission");
}

#[test]
fn monitor_and_solver_values_keep_bits_across_json_sqlite_roundtrips() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute("CREATE TABLE receipt (id INTEGER PRIMARY KEY, payload TEXT NOT NULL)", []).unwrap();
    let expected = [0.0015707963267948967_f64, 115.53058487056117_f64, 449.9999999962708_f64,
                    0.14694635007449575_f64, -0.0_f64];
    let mut receipt = serde_json::json!({"values":expected});
    for generation in 0..8 {
        let raw = serde_json::to_string(&receipt).unwrap();
        connection.execute("INSERT OR REPLACE INTO receipt (id,payload) VALUES (1,?1)", [&raw]).unwrap();
        let retained: String = connection.query_row("SELECT payload FROM receipt WHERE id=1", [], |row| row.get(0)).unwrap();
        receipt = serde_json::from_str(&retained).unwrap();
        for (index, value) in expected.iter().enumerate() {
            assert_eq!(receipt["values"][index].as_f64().unwrap().to_bits(), value.to_bits(),
                "Stored scientific number changed at generation {generation}, index {index}");
        }
    }
}
