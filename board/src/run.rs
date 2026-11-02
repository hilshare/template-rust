//! The timer, the stack paint and the check lines of a run. Your checks go in `main.rs`.

// `main.rs` calls only some of the forms of a check.
#![allow(dead_code)]

use core::fmt::{Display, Formatter, Write};

use libtock::platform::Syscalls;
use libtock::runtime::{stack_size, TockSyscalls};

/// Must equal `--stack` in build-tab.sh.
const STACK_SIZE: usize = 0x4000;
stack_size! {STACK_SIZE}

/// `capsules_extra::cycle_count::DRIVER_NUM`, and its commands.
const CYCLE_COUNT_DRIVER: u32 = 0x90008;
const CYCLE_COUNT_START: u32 = 1;
const CYCLE_COUNT_READ: u32 = 2;
const CYCLE_COUNT_RESET: u32 = 3;
/// The clock driver of the HILShare kernel. Command 1 returns the CPU clock in hertz.
const CLOCK_RATE_DRIVER: u32 = 0xA0000;
const CLOCK_RATE_HERTZ: u32 = 1;

/// Written over the free stack before a task. The deepest word that differs after the task
/// is its peak.
const STACK_PATTERN: u32 = 0xA5A5_A5A5;
/// Kept free below the frame of `paint_stack`, so the fill does not write a live frame.
const STACK_PAINT_MARGIN: usize = 256;

/// A hexadecimal number with eight digits.
#[derive(PartialEq)]
pub struct Hexadecimal(pub u32);

impl Display for Hexadecimal {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{:08x}", self.0)
    }
}

/// The cycles and the peak stack of one task.
struct Measurement {
    cycles: u32,
    stack_bytes: usize,
}

/// The cycle counter, the CPU clock, and the round trip of one counter read.
struct Timer {
    clock_hz: u32,
    overhead_cycles: u32,
}

impl Timer {
    /// Starts the cycle counter and measures the round trip. `Err` holds the check name of
    /// the driver that is not available.
    fn start() -> Result<Self, &'static str> {
        let clock_hz = TockSyscalls::command(CLOCK_RATE_DRIVER, CLOCK_RATE_HERTZ, 0, 0)
            .get_success_u32()
            .ok_or("clock_rate")?;
        let _ = TockSyscalls::command(CYCLE_COUNT_DRIVER, CYCLE_COUNT_RESET, 0, 0);
        if !TockSyscalls::command(CYCLE_COUNT_DRIVER, CYCLE_COUNT_START, 0, 0).is_success() {
            return Err("cycle_counter");
        }
        let before = read_cycles();
        let after = read_cycles();
        Ok(Self {
            clock_hz,
            overhead_cycles: after.wrapping_sub(before),
        })
    }

    /// Runs `task` one time. A task passes its inputs through `black_box`, so its work
    /// stays between the two reads.
    fn measure<T>(&self, task: impl FnOnce() -> T) -> (T, Measurement) {
        let painted_end = paint_stack();
        let before = read_cycles();
        let result = core::hint::black_box(task());
        let after = read_cycles();
        let stack_bytes = peak_stack_bytes(painted_end);
        let cycles = after
            .wrapping_sub(before)
            .saturating_sub(self.overhead_cycles);
        (
            result,
            Measurement {
                cycles,
                stack_bytes,
            },
        )
    }

    /// `cycles × 100 000 000 / clock_hz`.
    fn hundredths_of_a_microsecond(&self, cycles: u32) -> u64 {
        u64::from(cycles) * 100_000_000 / u64::from(self.clock_hz)
    }
}

/// `CYCCNT` is 32 bits. `wrapping_sub` holds across one wrap.
fn read_cycles() -> u32 {
    TockSyscalls::command(CYCLE_COUNT_DRIVER, CYCLE_COUNT_READ, 0, 0)
        .get_success_u64()
        .map_or(0, |count| count as u32)
}

/// Fills `STACK_PATTERN` from the stack limit to `STACK_PAINT_MARGIN` below this frame, and
/// returns the end of the fill.
#[inline(never)]
fn paint_stack() -> usize {
    let frame_marker = 0u8;
    let frame = core::hint::black_box(core::ptr::addr_of!(frame_marker)) as usize;
    let start = core::ptr::addr_of_mut!(STACK_MEMORY) as usize;
    let end = (frame - STACK_PAINT_MARGIN) & !3;
    let mut address = start;
    while address < end {
        // SAFETY: [start, end) is in STACK_MEMORY, below every live frame.
        unsafe { core::ptr::write_volatile(address as *mut u32, STACK_PATTERN) };
        address += 4;
    }
    end
}

/// From the stack top to the deepest word that does not hold `STACK_PATTERN`. A task that
/// stayed above the fill gives the depth of the fill end.
fn peak_stack_bytes(painted_end: usize) -> usize {
    let start = core::ptr::addr_of!(STACK_MEMORY) as usize;
    let mut address = start;
    // SAFETY: [start, painted_end) is in STACK_MEMORY.
    while address < painted_end
        && unsafe { core::ptr::read_volatile(address as *const u32) } == STACK_PATTERN
    {
        address += 4;
    }
    start + STACK_SIZE - address
}

/// One run: the console, the timer, and the tally of the checks.
pub struct Run<Output: Write> {
    console: Output,
    timer: Timer,
    passed: usize,
    total: usize,
}

impl<Output: Write> Run<Output> {
    /// Prints the start markers and starts the timer. `None` after it printed the failed
    /// check and the result.
    #[must_use]
    pub fn start(mut console: Output) -> Option<Self> {
        let _ = writeln!(console, "HILSHARE_START_EXECUTION");
        let _ = writeln!(console, "HILSHARE_FIRMWARE={}", env!("CARGO_PKG_VERSION"));
        match Timer::start() {
            Ok(timer) => Some(Self {
                console,
                timer,
                passed: 0,
                total: 0,
            }),
            Err(missing) => {
                let _ = writeln!(console, "HILSHARE_CHECK name={missing} status=FAIL");
                let _ = writeln!(console, "HILSHARE_RESULT=FAIL");
                None
            }
        }
    }

    /// Passes if the result of `task` equals `expected`. `name` and the `Display` form of the
    /// result must hold no space.
    pub fn check<Answer: PartialEq + Display>(
        &mut self,
        name: &str,
        expected: Answer,
        task: impl FnOnce() -> Answer,
    ) {
        let (answer, measurement) = self.measure(name, task);
        let equal = answer == expected;
        self.report(name, equal, &answer, &expected, &measurement, None);
    }

    /// Passes if `|result − expected| ≤ tolerance`. A result that is not a number fails.
    pub fn check_close(
        &mut self,
        name: &str,
        expected: f64,
        tolerance: f64,
        task: impl FnOnce() -> f64,
    ) {
        let (answer, measurement) = self.measure(name, task);
        let close = (answer - expected).abs() <= tolerance;
        self.report(name, close, &answer, &expected, &measurement, None);
    }

    /// Passes if the result of `task` equals `expected` and the time is not above
    /// `budget_microseconds`.
    pub fn check_within_budget<Answer: PartialEq + Display>(
        &mut self,
        name: &str,
        expected: Answer,
        budget_microseconds: u32,
        task: impl FnOnce() -> Answer,
    ) {
        let (answer, measurement) = self.measure(name, task);
        let equal = answer == expected;
        let budget = Some(budget_microseconds);
        self.report(name, equal, &answer, &expected, &measurement, budget);
    }

    /// Prints the name first, so that the log names the check after a hang or a panic.
    fn measure<Answer>(
        &mut self,
        name: &str,
        task: impl FnOnce() -> Answer,
    ) -> (Answer, Measurement) {
        let _ = writeln!(self.console, "running {name}");
        self.timer.measure(task)
    }

    /// Prints the check line. `expected=` is on the line only if the answer is incorrect.
    fn report(
        &mut self,
        name: &str,
        answer_is_correct: bool,
        answer: &impl Display,
        expected: &impl Display,
        measurement: &Measurement,
        budget_microseconds: Option<u32>,
    ) {
        let time = self.timer.hundredths_of_a_microsecond(measurement.cycles);
        let overhead = self
            .timer
            .hundredths_of_a_microsecond(self.timer.overhead_cycles);
        let passed = answer_is_correct
            && budget_microseconds.is_none_or(|budget| time <= u64::from(budget) * 100);
        let status = if passed { "PASS" } else { "FAIL" };
        let _ = write!(
            self.console,
            "HILSHARE_CHECK name={name} status={status} answer={answer}"
        );
        if !answer_is_correct {
            let _ = write!(self.console, " expected={expected}");
        }
        let _ = write!(
            self.console,
            " cycles={} us={}.{:02} overhead_us={}.{:02}",
            measurement.cycles,
            time / 100,
            time % 100,
            overhead / 100,
            overhead % 100
        );
        if let Some(budget) = budget_microseconds {
            let _ = write!(self.console, " budget_us={budget}");
        }
        let _ = writeln!(self.console, " stack_bytes={}", measurement.stack_bytes);
        self.total += 1;
        self.passed += usize::from(passed);
    }

    /// Prints the tally and the result.
    pub fn finish(mut self) {
        let _ = writeln!(
            self.console,
            "HILSHARE_CHECKS_PASSED={}/{}",
            self.passed, self.total
        );
        let _ = writeln!(
            self.console,
            "HILSHARE_RESULT={}",
            if self.passed == self.total {
                "PASS"
            } else {
                "FAIL"
            }
        );
    }
}
