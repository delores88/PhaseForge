"""Read-only numerical dataset export in the pinned LPAC NumPy runtime.

No model fitting, label synthesis, extrapolation or network access. The trusted
study coordinator admits this stage only after frozen held-out evaluation.
"""
import hashlib
import json
from pathlib import Path
import numpy as np


def read(spec):
    relative = spec["path"]
    parts = relative.split("/")
    assert relative and not any(part in ("", ".", "..") for part in parts)
    assert ":" not in relative and "\\" not in relative
    path = Path("imports").joinpath(*parts)
    raw = path.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == spec["sha256"]
    return json.loads(raw, parse_constant=lambda text: (_ for _ in ()).throw(ValueError(text)))


request = json.loads(Path("input.json").read_text(encoding="utf-8"))
split = read(request["split"])
roles = ["train", "validation", "calibration", "test", "regime_test", "ood"]
conditions = split["conditions"] + [split["ood"]]
assert len(conditions) == 33
seeds = {}
for spec in request["sources"]:
    shard = read(spec)
    for key in ("study_id", "proposal_sha256", "protocol_sha256"):
        assert shard[key] == split[key]
    assert shard["split_sha256"] == request["split"]["sha256"]
    assert shard["role"] == spec["role"]
    for row in shard["seeds"]:
        index, replica = row["condition_index"], row["replicate_index"]
        assert type(index) is int and 0 <= index < 33 and type(replica) is int and 0 <= replica < 3
        assert (index, replica) not in seeds
        condition = conditions[index]
        assert row["role"] == condition["role"] == shard["role"]
        assert row["seed"] == 110000 + 1000 * index + replica
        assert row["temperature_kelvin"] == condition["temperature_kelvin"]
        assert row["density_g_cm3"] == condition["density_g_cm3"]
        seeds[index, replica] = row
assert set(seeds) == {(index, replica) for index in range(33) for replica in range(3)}
ordered = [seeds[index, replica] for index in range(33) for replica in range(3)]
pressure = np.array([row["pressure_bar"] for row in ordered], dtype=np.float64).reshape(33, 3)
p0 = np.array([row["p0_bar"] for row in ordered], dtype=np.float64).reshape(33, 3)
assert np.isfinite(pressure).all() and np.isfinite(p0).all() and (p0 > 0).all()
assert np.max(np.abs(p0 - p0[:, :1])) <= 1e-9
means = pressure.mean(axis=1)
sd = pressure.std(axis=1, ddof=1)
np.savez("dataset.npz",
         condition_index=np.arange(33, dtype=np.int64),
         temperature_kelvin=np.array([row["temperature_kelvin"] for row in conditions], dtype=np.float64),
         density_g_cm3=np.array([row["density_g_cm3"] for row in conditions], dtype=np.float64),
         role_code=np.array([roles.index(row["role"]) for row in conditions], dtype=np.int64),
         replicate_seeds=np.array([row["seed"] for row in ordered], dtype=np.int64).reshape(33, 3),
         seed_pressure_bar=pressure, mean_pressure_bar=means, seed_sd_bar=sd,
         seed_se_bar=sd / np.sqrt(3), p0_bar=p0[:, 0], Z=means / p0[:, 0])
metadata = {"schema_version": 1, "study_id": split["study_id"],
            "proposal_sha256": split["proposal_sha256"], "protocol_sha256": split["protocol_sha256"],
            "split_sha256": request["split"]["sha256"], "source_shards": request["sources"],
            "conditions": conditions, "seeds": ordered, "role_codes": dict(enumerate(roles)),
            "dataset_sha256": hashlib.sha256(Path("dataset.npz").read_bytes()).hexdigest(),
            "units": {"temperature": "K", "density": "g/cm^3", "pressure": "bar", "Z": "1"},
            "scope": "33 computational conditions, three seeds each; OOD is excluded from all original fitting and held-out scores"}
Path("dataset.json").write_text(json.dumps(metadata, sort_keys=True, allow_nan=False), encoding="utf-8")
Path("result.json").write_text(json.dumps({"status": "exported", "conditions": 33, "seeds": 99,
    "artifacts": ["dataset.npz", "dataset.json"], "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}), encoding="utf-8")
