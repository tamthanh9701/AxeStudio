## Ticket

<!-- Link issue ALS-XXX -->

## Thay đổi

## Definition of Done

- [ ] `cargo fmt --check` và `clippy -D warnings` sạch
- [ ] Test mới phủ đúng acceptance của ticket
- [ ] Không chạm file ngoài danh sách "File được đụng"
- [ ] Bindings regenerate nếu đổi kiểu Rust (`pnpm bindings:generate`)
- [ ] Không vượt ngân sách `docs/perf-budget.md`
- [ ] Đổi kiến trúc → có ADR mới
- [ ] Đổi contract → PR này CHỈ chứa thay đổi contract
