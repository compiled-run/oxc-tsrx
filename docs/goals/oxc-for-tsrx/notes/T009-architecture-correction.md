# T009 architecture correction

T009 proved that a one-call native TSRX formatter can exceed the frozen speed,
latency, memory, correctness, and packaging controls. That evidence remains
useful, but the owner rejected Zig/Yuku as the production center of a project
whose purpose is genuine OXC integration.

The prototype is therefore not a shippable product slice. The production core
must be Rust, consume published version-pinned OXC crates behind a narrow
compatibility adapter, and leave JavaScript/TypeScript only at npm, Vite/Vite+,
configuration, and editor boundaries. Yuku remains a performance oracle only.

No external repository was modified. Existing prototype files stay in this
workspace temporarily so their red/green fixtures and measurements can be used
as differential controls. They must be removed or replaced after the Rust seam
has equivalent retained evidence; they may not be documented or packaged as
the final architecture.
