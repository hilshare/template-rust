# HILShare template for Rust

This repository holds a Tock app in Rust for a [HILShare](https://github.com/hilshare/action) board.
Each push to `main` builds the app on a hosted runner and runs it on a physical Nucleo-F411RE. The
result is a check on your commit. You need no board, no token and no secret.

## Use

For a new repository:

1. Select **Use this template** on GitHub. The first push starts a run.
2. Replace `TEXT`, `crc32` and the check in `board/src/main.rs` with your own checks.
3. Add your own copyright line for your own code. `LICENSE` stays with this code.

For a repository that you already have:

1. Copy `board/` and `.github/workflows/board.yml` into your repository.
2. Add your crate to `[dependencies]` in `board/Cargo.toml`. Many crates need
   `default-features = false` for `no_std`.

   ```toml
   my-library = { path = "..", default-features = false }
   ```

3. If the `Cargo.toml` at the root of your repository has a `[workspace]` table, add
   `exclude = ["board"]` to that table.
4. Add your checks to `board/src/main.rs`.

`board/` has its own toolchain, `Cargo.lock` and `.cargo/config.toml`, so the build of your library
does not change. The name `my-app` can stay. To change it, change `board/Cargo.toml`,
`board/build-tab.sh` and the three paths in `board.yml`.

## Build

Install elf2tab 0.13.0:

```sh
cargo install elf2tab --version 0.13.0 --locked
```

Then build the TAB from `board/`:

```sh
./build-tab.sh
```

The TAB is `board/target/tab/my-app.tab`. `board/rust-toolchain.toml` selects Rust 1.92.
`board/.cargo/config.toml` gives the fixed addresses of the board: app flash from `0x08040000` and
app RAM from `0x20008000`. `build-tab.sh` builds the TBF for kernel version 2.3, the version that the
Tock release-2.3.0 kernel reports. A TAB with other values fails preflight, and the sentence names the
correction.

## What a run can test

A run tests code that computes. The kernel gives an app a console, a cycle counter, the clock rate
and an alarm. The app runs unprivileged, in its own flash and RAM. It cannot use a peripheral, an
interrupt or a register of the core, and such an access faults. A driver, an RTOS or a HAL does not
run here.

| Limit | Value |
|---|---|
| Flash | 255 KB |
| RAM | 60 KB for the stack, the heap and the static data. The link fails above this limit |
| Stack | 16 KB. To change it, change `STACK_SIZE` in `board/src/run.rs` and `--stack` in `build-tab.sh` together |
| Heap | 8 KB, with `ARENA_SIZE` in `board/src/allocator.rs`. The allocator never frees |
| Time | 10 s, about 10⁹ cycles. A sleep counts |
| Console | A check takes about 8 ms, so one run holds about 1200 checks |
| Test data | Your data goes in flash, for example with `include_bytes!`. A run has no input |

Floating-point math runs in software, because Tock does not save the FPU state on a Cortex-M. The
results are correct, and the code takes more time than firmware that uses the FPU. Do not build for
`thumbv7em-none-eabihf`, because an FPU instruction faults. `usize` has 32 bits, so a value that you
copy from a 64-bit host can differ.

## Write your checks

`cargo test` does not run on the board. Copy each test into one check:

| Call | The check passes if |
|---|---|
| `run.check(name, expected, task)` | The result equals `expected` |
| `run.check_close(name, expected, tolerance, task)` | `|result − expected| ≤ tolerance`, for `f64` |
| `run.check_within_budget(name, expected, budget_microseconds, task)` | The result equals `expected`, and the time is not above the budget |

- `task` runs one time, and each call prints a line with the time and the stack.
- The result must have `PartialEq` and `Display`. To test more than one value, return a `bool`:

  ```rust
  run.check("parse", true, || parse(b"1") == 1 && parse(b"2") == 2);
  ```

- To test many values in one line of the log, return a count of the failures:

  ```rust
  run.check("vectors", 0, || VECTORS.iter().filter(|v| parse(v.input) != v.output).count());
  ```

- A name and a result cannot hold a space. Each one that does ends the run at exit 7.
- If an answer is incorrect, the line also holds `expected=`.
- To find an expected value, run the check with any value, then read `answer` from the log.

To test the `sha2` crate:

1. Add the crate to `[dependencies]` in `board/Cargo.toml`:

   ```toml
   sha2 = { version = "=0.11.1", default-features = false }
   ```

2. Add this line to `board/src/main.rs`, below the other `use` lines:

   ```rust
   use sha2::{Digest, Sha256};
   ```

3. Add one check to `main`, before `run.finish()`. `black_box` keeps the input out of the reach of
   the optimizer. The expected value is the first four bytes of the SHA-256 of `abc`.

   ```rust
   run.check("sha256_abc", Hexadecimal(0xBA78_16BF), || {
       let digest = Sha256::digest(core::hint::black_box(b"abc"));
       Hexadecimal(u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]))
   });
   ```

4. Push. The job log shows the check line.

## Build errors

| Text in the build log | Correction |
|---|---|
| `can't find crate for std` | A dependency uses `std`. Set `default-features = false`. Many crates then need `features = ["alloc"]` |
| `requires rustc` | The crate needs a newer Rust. Change `channel` in `rust-toolchain.toml` and `rust-version` in `Cargo.toml`, then measure each budget again |
| `will not fit in region` | The stack, the heap and the static data are above 60 KB. Make `ARENA_SIZE` or the stack smaller |
| `no method named`, for `sqrt` or another math function | `core` has no such math. Use the `libm` crate, or `num-traits` with its `libm` feature |
| `AtomicU64` not found | This core has no 64-bit atomics. Use `portable-atomic` with the features `fallback` and `unsafe-assume-single-core`. A Tock process has one thread |
| `target is not supported` from `getrandom` | The board has no source of random numbers. Give the library a fixed seed, and disable its `getrandom` feature |
| A failure in a crate that compiles C | Add a step with `sudo apt-get install --yes gcc-arm-none-eabi` to `board.yml` |

## Read a failure

| Exit | Meaning | What to do |
|---|---|---|
| 1 | The app reported `FAIL` | Read the check lines |
| 2 | The TAB fails preflight | The sentence names the correction |
| 5 | The kernel did not load the app, or the app did not start a run | The sentence gives the reason of the kernel. With no reason, look for a fault in code that runs before `main` |
| 6 | The run did not end | A panic prints its message and its file. After a hang, the last `running` line names the check |
| 7 | A line that HILShare cannot read | A name or a result holds a space |
| 8 | Three attempts gave no verdict | The app printed no `HILSHARE_START_EXECUTION` and did not exit. Look for a hang before the first check |
| 9 | The app faulted | Read `reason`, `addr` and `pc` on the fault line |

A panic ends the run, so the later checks do not run. The check lines above the panic stay in the
log.

To find the source line of a fault, download the artifact `my-app-elf` from the workflow run, then run
this command with the value of `pc`. `-i` also prints each inlined function, so `main` is in the output:

```sh
arm-none-eabi-addr2line -f -i -C -e my-app <pc>
```

A stack overflow writes below the app RAM, so it gives exit 9. `reason` is `mpu_stacking` or
`mpu_data`. If the fault line has `addr`, its value is just below `0x20008000`. The line has no `pc`,
because the stack holds no exception frame.

## Use your runs

An owner has 20 runs in a day, for all its repositories. Each push to `main` and each push to a pull
request uses one run. A new push to a pull request replaces its queued job, and a job that does not
start uses no run. To keep a commit from a run, add `[skip ci]` to its message. Do not use
`paths-ignore` with a required check, because GitHub leaves that check pending for a skipped workflow.

## The rules of a run

- A run has 10 s from the boot line to the exit line, and a sleep counts. The run ends after that
  time.
- `cycles` must not time a section that sleeps. The CPU sleeps in `WFI`, and the debug probe controls
  whether `CYCCNT` counts there. Time such a section with the ticks of the alarm driver, driver
  number `0x0`. Command 1 of that driver gives the tick rate.
- A probe reset halts the core for about 24 ms. The alarm counts through the halt, and `CYCCNT` does
  not. An app that compares the two must wait 100 ms after the reset.
- A timed section starts from a cold cache. To time it warm, run it one time before you time it.
- The kernel runs a timer at the end of each 100 ms timeslice, so a task above 100 ms includes kernel
  time. The `timeslices` field of the exit line gives the count.
- A pull request from a fork or from Dependabot gets no OIDC token. Its check is the build, and its
  board run comes after the merge. Refer to `.github/workflows/board.yml`.

The app prints the lines of the [HILShare contract](https://github.com/hilshare/action/blob/main/SPECS.md).
The app decides `PASS` or `FAIL`. HILShare copies that verdict to the exit code. The
[action](https://github.com/hilshare/action) gives the exit codes and the limits, and each run accepts
the [terms](https://github.com/hilshare/action/blob/main/TERMS.md).

## Output

A run on a Nucleo-F411RE with HILShare kernel `0.2.0`. `␞` is the byte `0x1E`. The kernel prints the
first line and the last line, and the app prints the other lines.

```
␞HILSHARE_KERNEL_BOOT version=0.2.0 clock_hz=100000000
HILSHARE_START_EXECUTION
HILSHARE_FIRMWARE=0.1.0
running crc32
HILSHARE_CHECK name=crc32 status=PASS answer=414fa339 cycles=1295 us=12.95 overhead_us=15.31 budget_us=15 stack_bytes=512
HILSHARE_CHECKS_PASSED=1/1
HILSHARE_RESULT=PASS
␞HILSHARE_KERNEL_APP_EXIT name=my-app code=0 flash=16384 ram=32768 stack=472 heap=0 grants=1196 syscalls=215 timeslices=0
```

## Budgets

A budget is optional. Each budget here is `ceil(1.1 × us)` of the figure on a Nucleo-F411RE with
HILShare kernel `0.2.0`. Measure each budget again after a change to the toolchain, the kernel
version, `Cargo.lock`, the release profile or `.cargo/config.toml`. A kernel change can turn a budget
red with no commit of yours, and the release notes of the
[action](https://github.com/hilshare/action/releases) give each kernel change.

In about one run in ten, `overhead_us` is up to 0.7 µs higher, and `us` is lower by the same amount.
Set a budget from the value that most runs give.

| Task | `us` | `budget_us` |
|---|---|---|
| `crc32` | 12.95 | 15 |

## Licence

Apache License 2.0. Refer to [LICENSE](LICENSE).
