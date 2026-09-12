//! The agent and user interface share one executable capability catalog.
use serde_json::{json,Value};

pub fn capabilities()->Value {
    json!({"engines":[argon_capability(),super::field::capability(),super::mechanics::capability(),super::thermal::capability(),super::fluid::capability()],
        "gaps":[
            {"domain":"Relativistic black-hole mergers","status":"not_installed","reason":"No Einstein-equation evolution engine, constraint-solving initial data, horizon finder or gravitational-wave extraction is integrated.","available_alternative":"Newtonian point-mass dynamics applies only in a weak-field, slow-motion regime. Incoming shock-limit fields or published simulation replays are separate calculations, not a newly solved merger.","required":"A compatible numerical-relativity engine and validated initial data, resolution/convergence checks and suitable compute."},
            {"domain":"General thermodynamics","status":"bounded","reason":"Heat conduction and classical Lennard-Jones molecular measurements are available. No universal equation of state, phase-equilibrium, reacting-flow or material-property database is bundled."},
            {"domain":"General fluid dynamics","status":"bounded","reason":"The incompressible solver covers periodic two-dimensional viscous flow. It does not cover compressible shocks, arbitrary walls, free surfaces, multiphase flow or turbulence closures."},
            {"domain":"Predictive biological experiments","status":"not_installed","reason":"No validated general infection, drug-binding, efficacy or clinical model is bundled. An illustrative structure is not evidence of those mechanisms."}
        ],
        "generated_experiments":"Retained Python/NumPy instruments can implement additional explicit mathematical models. Their execution, numerical validation and real-world predictive validity are separate claims.",
        "policy":"Simulation requests require retained temporal numerical evidence or an explicit capability gap. A still image does not satisfy a simulation request."})
}

fn argon_capability()->Value{
    json!({"id":"openmm_argon","engine_version":"OpenMM 8.5.2","scope":"Periodic classical monatomic argon-like Lennard-Jones fluid. Does not model proteins, reactions or biological efficacy.",
        "parameters":{"atom_count":{"default":108,"min":32,"max":512},"temperature_kelvin":{"default":120,"min":20,"max":500},"density_g_cm3":{"default":1.0,"min":0.02,"max":2.0},"steps":{"default":2000,"min":1,"max":5000000},"timestep_fs":{"default":1.0,"min":0.1,"max":5.0},"seed":{"default":20260911,"min":1,"max":2147483647},"platform":{"default":"CPU","allowed":["Reference","CPU","CUDA","OpenCL"]},"thermostat":{"default":"langevin","allowed":["langevin","nve"]},"friction_per_ps":{"default":1.0,"min":0.01,"max":100,"note":"Validated but inactive for NVE; retain default instead of zero"},"sample_interval":{"default":20,"min":1,"max":1000},"chunk_frames":{"default":50,"min":1,"max":100},"cpu_threads":{"default":1,"min":1,"max":8}},
        "limits":{"retained_frames":50001,"atom_steps":50000000,"maximum_sample_period_ps":1.0},"instruments":["energy","temperature","pressure","radial distribution","mean squared displacement"],"acceleration":"Requested OpenMM platform; unsupported platform fails explicitly","observations":"Hash-linked PNG projections and contemporaneous numerical instruments at committed checkpoints"})
}
