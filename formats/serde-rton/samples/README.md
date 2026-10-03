# Curated RTON Samples

This directory contains a small representative subset of PvZ2 RTON files used
for regression testing and examples. The source corpus is much larger, so these
fixtures are intentionally selected to keep the repository lightweight while
covering common structures seen in real data.

| File | Why it is included |
| ---- | ------------------ |
| `rton/PACKAGES/VERSION.RTON` | Tiny file for smoke tests and minimal object handling. |
| `rton/PACKAGES/COLORS.RTON` | Compact numeric table with many repeated string keys. |
| `rton/PACKAGES/CREATURETYPES.RTON` | Small file containing RTID values. |
| `rton/PACKAGES/HOTUI/ALMANACWIDGET.RTON` | UI layout data with RTIDs, bools, floats, arrays, and duplicate keys. |
| `rton/PACKAGES/LEVELS/EGYPT16.RTON` | Level data with nested arrays/objects, many RTIDs, and duplicate keys. |
| `rton/PACKAGES/LEVELMUTATORMODULES.RTON` | Module data with RTIDs and Latin-1 high-byte strings. |
| `rton/PACKAGES/NEWS.RTON` | Text-heavy data with Latin-1 high-byte strings. |
| `rton/PACKAGES/THYMED_EVENT_SCHEDULE.RTON` | Deeply nested schedule data. |
| `rton/PACKAGES/PLANTLEVELS.RTON` | Large numeric progression table with many arrays and floats. |
| `rton/PACKAGES/PROJECTILETYPES.RTON` | Large property table with floats and duplicate keys. |

These samples are covered by `tests/curated_samples.rs`, which verifies:

- `RTON -> Value`
- `RTON -> Value -> standard RTON -> Value`
- `RTON -> Value -> compact RTON -> Value`
- `RTON -> Value -> serde_json -> Value -> RTON -> Value`
