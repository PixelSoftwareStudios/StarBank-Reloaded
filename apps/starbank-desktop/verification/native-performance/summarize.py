import argparse
import json
import statistics
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("samples", type=Path)
parser.add_argument("timebase", help="mach_timebase_info numerator/denominator in nanoseconds per tick")
parser.add_argument("--historical-counter-labels", action="store_true", help="read original mislabelled *_ns fields as ticks")
args = parser.parse_args()
numerator, denominator = map(int, args.timebase.split("/"))
ns_per_tick = numerator / denominator
counter_label = "ns" if args.historical_counter_labels else "ticks"
mib = 1024**2

def span(values, digits=0):
    return f"{statistics.median(values):.{digits}f} ({min(values):.{digits}f}–{max(values):.{digits}f})"

records = [json.loads(line) for line in args.samples.read_text().splitlines()]
assert len(records) == 18
assert len({record["app_pid"] for record in records}) == len(records)
assert not [record for record in records if record["app_child_names"]]
for scenario in ("empty", "small", "large"):
    relevant = [record for record in records if record["case"] == scenario]
    warmup, *measured = sorted(relevant, key=lambda record: record["iteration"])
    assert warmup["warmup"] and len(measured) == 5
    assert all(not record["warmup"] and len(record["app_samples"]) == 7 for record in measured)
    print(scenario, "values", warmup["values"], "file bytes", warmup["file_bytes"])
    for metric in ("window_ms", "ui_ms", "open_ms", "idle_interval_ms"):
        if metric in warmup:
            print(" ", metric, span([record[metric] for record in measured]), "warmup", round(warmup[metric], 1))
    for metric in ("resident_bytes", "physical_footprint_bytes"):
        values = [[sample[metric] / mib for sample in record["app_samples"]] for record in measured]
        print(" ", metric, "median across run medians", span([statistics.median(run) for run in values], 1),
              "all sample range", f"{min(map(min, values)):.1f}–{max(map(max, values)):.1f}")
    maxima = [record["app_lifetime_max_physical_footprint_bytes"] / mib for record in measured]
    print(" ", "app_lifetime_max_physical_footprint_MiB", span(maxima, 1))
    idle_cpu_ms = [(record[f"app_idle_user_{counter_label}"] + record[f"app_idle_system_{counter_label}"]) * ns_per_tick / 1e6 for record in measured]
    print(" ", "idle_cpu_ms", span(idle_cpu_ms, 2))
    print(" ", "idle_cpu_pct_one_core", span([value * 100 / record["idle_interval_ms"] for value, record in zip(idle_cpu_ms, measured)], 3))
