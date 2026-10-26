//! This computer's own meters, CPU, RAM, GPU and VRAM, as the built-in
//! service PC. A thread measures each second those that show, and nothing
//! while none does, with Windows' performance counters, counted as Task
//! Manager counts. It opens no Direct3D device. RAM and VRAM also tell their
//! amounts.

use crate::choice::Choice;
use crate::log::{Log, log};
use crate::usage::{Amount, Limit, Meaning, Reading, Service};
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::Performance::*;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::core::{PCWSTR, w};

pub const ID: &str = "pc";
const NAME: &str = "PC";
/// Its own color, apart from every other service's.
pub const COLOR: u32 = 0xFFA6_D96A;
/// The meters, in the order they are listed.
pub const METERS: [&str; 4] = ["CPU", "RAM", "GPU", "VRAM"];
const CPU: usize = 0;
const RAM: usize = 1;
const GPU: usize = 2;
const VRAM: usize = 3;

/// How often the meters shown are measured, and how often, with none
/// shown, it looks whether one is.
const EVERY: Duration = Duration::from_secs(1);
const LOOK: Duration = Duration::from_millis(250);
/// Two samples further apart than this, as across sleep, give a share over
/// that whole time: it is not shown as now.
const GAP: Duration = Duration::from_secs(5);
/// A GPU with less memory of its own than this is built in and uses the
/// computer's: its VRAM is the computer's memory it shares.
const OWN_MEMORY: u64 = 512 << 20;
/// How long after it is missed the GPU chosen is looked for again.
const SEEK: Duration = Duration::from_secs(10);
/// How many CPU samples in a row may roll over before it counts as failing.
const MOST_ROLLED: u32 = 5;

/// Task Manager's CPU share, which counts the processor's speed as well as
/// its busy time; the share each process has of each GPU engine; and each
/// GPU's memory of its own in use, and the computer's it shares, in bytes.
/// English names hold in any Windows language.
const CPU_COUNTER: PCWSTR = w!("\\Processor Information(_Total)\\% Processor Utility");
const ENGINES_COUNTER: PCWSTR = w!("\\GPU Engine(*)\\Utilization Percentage");
const MEMORY_COUNTER: PCWSTR = w!("\\GPU Adapter Memory(*)\\Dedicated Usage");
const SHARED_COUNTER: PCWSTR = w!("\\GPU Adapter Memory(*)\\Shared Usage");

/// PC with these readings and amounts, one for each meter.
pub fn service(readings: [Reading; 4], amounts: [Option<Amount>; 4]) -> Service {
    Service {
        id: ID,
        name: NAME.into(),
        color: COLOR,
        limits: Some(
            METERS
                .iter()
                .zip(readings)
                .zip(amounts)
                .map(|((label, reading), amount)| Limit {
                    label: (*label).into(),
                    reading,
                    resets: None,
                    amount,
                })
                .collect(),
        ),
        trouble: None,
        meaning: Meaning::Meter,
    }
}

/// Measures until `stop` is dropped, handing `put` PC with what it measured
/// each time.
pub fn run(stop: &mpsc::Receiver<()>, file: &Log, choice: &Choice, put: impl Fn(Service)) {
    // Listed from the start, so each meter can be picked before it shows.
    put(service([Reading::Pending; 4], [None; 4]));
    let mut meters = Meters::default();
    let mut was = [false; 4];
    let mut panicked = false;
    loop {
        let want = wanted(choice).unwrap_or_default();
        if want != was {
            log(file, told(want));
            was = want;
        }
        let wait = if want.contains(&true) {
            // A panic in a sample fails the meters, and the counters open
            // again, so that the thread goes on measuring.
            let sampled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                meters.sample(want, file)
            }));
            let (readings, amounts) = match sampled {
                Ok(sampled) => {
                    panicked = false;
                    sampled
                }
                Err(_) => {
                    if !panicked {
                        log(file, "pc: measuring panicked; the counters open again");
                    }
                    panicked = true;
                    meters.close();
                    ([Reading::Failed(None); 4], [None; 4])
                }
            };
            put(service(readings, amounts));
            EVERY
        } else {
            if meters.close() {
                put(service([Reading::Pending; 4], [None; 4]));
            }
            LOOK
        };
        if !matches!(
            stop.recv_timeout(wait),
            Err(mpsc::RecvTimeoutError::Timeout)
        ) {
            return;
        }
    }
}

/// Which meters show: those any box shows, and, while the popup is open,
/// those its summary shows. None until the controller has offered PC.
fn wanted(choice: &Choice) -> Option<[bool; 4]> {
    let offers = choice.offers();
    let offer = offers.iter().find(|o| o.id == ID)?;
    let mut labels = choice.all_limits(offer);
    if choice.popup_open() && choice.summary(&offers).contains(&ID) {
        labels.extend(choice.summary_limits(offer));
    }
    Some(METERS.map(|meter| labels.iter().any(|l| l == meter)))
}

/// The log's line when what is measured changes.
fn told(want: [bool; 4]) -> String {
    let named: Vec<&str> = METERS
        .iter()
        .zip(want)
        .filter(|(_, on)| *on)
        .map(|(meter, _)| *meter)
        .collect();
    if named.is_empty() {
        "pc: not shown; not measured until it is".into()
    } else {
        format!("pc: measuring {} each second", named.join(" "))
    }
}

/// A meter's share, and how much that is, when it tells one.
type Measured = (Option<u8>, Option<Amount>);

/// The counters open, the GPUs' memory as listed this sample, the GPU
/// chosen, and why each meter last failed, as the log was told.
struct Meters {
    cpu: Option<Counter>,
    held: Held,
    engines: Option<Counter>,
    memory: Option<Counter>,
    shared: Option<Counter>,
    listed: Option<Vec<(String, f64)>>,
    gpu: Gpu,
    last: Option<Instant>,
    failing: [Option<String>; 4],
}

impl Default for Meters {
    fn default() -> Self {
        Self {
            cpu: None,
            held: Held::default(),
            engines: None,
            memory: None,
            shared: None,
            listed: None,
            gpu: Gpu::Unchosen,
            last: None,
            failing: Default::default(),
        }
    }
}

/// The GPU measured: not chosen yet; chosen, while it is there, or since
/// when it has been missed; or none on this computer. It is chosen once a
/// run and never swapped for another.
enum Gpu {
    Unchosen,
    Chosen(Device, Option<Instant>),
    Missing,
}

impl Meters {
    /// Measures the meters wanted and closes the counters of the others.
    fn sample(&mut self, want: [bool; 4], file: &Log) -> ([Reading; 4], [Option<Amount>; 4]) {
        let now = Instant::now();
        let steady = self
            .last
            .is_some_and(|last| now.duration_since(last) <= GAP);
        self.last = Some(now);
        self.listed = None;
        let found = [
            want[CPU].then(|| self.cpu(steady, file).map(|share| (share, None))),
            want[RAM].then(ram),
            want[GPU].then(|| self.engines(steady, file).map(|share| (share, None))),
            want[VRAM].then(|| self.vram(file)),
        ];
        if !want[CPU] {
            self.cpu = None;
            self.held = Held::default();
        }
        if !want[GPU] {
            self.engines = None;
        }
        if !want[GPU] && !want[VRAM] {
            self.memory = None;
        }
        if !want[VRAM] {
            self.shared = None;
        }
        let mut readings = [Reading::Pending; 4];
        let mut amounts = [None; 4];
        for (at, found) in found.into_iter().enumerate() {
            let failing = match found {
                None => None,
                Some(Ok((share, amount))) => {
                    readings[at] = share.map_or(Reading::Pending, Reading::Read);
                    amounts[at] = amount;
                    None
                }
                Some(Err(why)) => {
                    readings[at] = Reading::Failed(None);
                    Some(why)
                }
            };
            if failing != self.failing[at] {
                match &failing {
                    Some(why) => log(
                        file,
                        format!("pc: {} could not be measured: {why}", METERS[at]),
                    ),
                    None if want[at] => log(file, format!("pc: {} measured again", METERS[at])),
                    None => {}
                }
                self.failing[at] = failing;
            }
        }
        (readings, amounts)
    }

    /// Closes every counter; whether any was open.
    fn close(&mut self) -> bool {
        let open = self.cpu.is_some()
            || self.engines.is_some()
            || self.memory.is_some()
            || self.shared.is_some();
        self.cpu = None;
        self.held = Held::default();
        self.engines = None;
        self.memory = None;
        self.shared = None;
        self.listed = None;
        self.last = None;
        self.failing = Default::default();
        open
    }

    /// Task Manager's CPU share; the last one again for a sample that rolled
    /// over, which the log tells.
    fn cpu(&mut self, steady: bool, file: &Log) -> Result<Option<u8>, String> {
        let Some(cpu) = Counter::rate(&mut self.cpu, CPU_COUNTER, steady)? else {
            return Ok(None);
        };
        let (share, rolled) = self.held.take(cpu.value())?;
        if let Some(status) = rolled {
            log(
                file,
                format!(
                    "pc: CPU counter rolled over ({}); its last value kept",
                    hex(status)
                ),
            );
        }
        Ok(share)
    }

    /// The GPU chosen's busiest engine, with each engine's share summed over
    /// the processes using it; 0 with none using it.
    fn engines(&mut self, steady: bool, file: &Log) -> Result<Option<u8>, String> {
        let Some(device) = self.device(file)? else {
            return Ok(None);
        };
        let luid = device.luid.clone();
        let Some(engines) = Counter::rate(&mut self.engines, ENGINES_COUNTER, steady)? else {
            return Ok(None);
        };
        Ok(Some(percent(
            busiest(&engines.items()?, &luid).unwrap_or(0.0),
        )))
    }

    /// The GPU chosen's memory in use, as a share of it: its own, or, for
    /// one built in, the computer's it shares, as Task Manager tells its
    /// shared GPU memory; none when it may share none.
    fn vram(&mut self, file: &Log) -> Result<Measured, String> {
        let Some(device) = self.device(file)? else {
            return Ok((None, None));
        };
        let (luid, shared) = (device.luid.clone(), device.built_in());
        let total = if shared { device.shared } else { device.memory };
        if total == 0 {
            return Ok((None, None));
        }
        let used = if shared {
            if self.shared.is_none() {
                self.shared = Some(Counter::open(SHARED_COUNTER)?);
            }
            let counter = self.shared.as_ref().ok_or("the counter is not open")?;
            counter.collect()?;
            in_use(&counter.items()?, &luid)
        } else {
            in_use(self.memory()?, &luid)
        }
        .unwrap_or(0.0);
        Ok((
            Some(percent(used / total as f64 * 100.0)),
            Some(Amount {
                used: used as u64,
                total,
                shared,
            }),
        ))
    }

    /// Each GPU's memory of its own in use, collected once a sample, from a
    /// counter opened when the GPU's meters first want it. It lists each GPU
    /// there is, so it also tells whether the one chosen is.
    fn memory(&mut self) -> Result<&[(String, f64)], String> {
        if self.listed.is_none() {
            if self.memory.is_none() {
                self.memory = Some(Counter::open(MEMORY_COUNTER)?);
            }
            let memory = self.memory.as_ref().ok_or("the counter is not open")?;
            memory.collect()?;
            self.listed = Some(memory.items()?);
        }
        Ok(self.listed.as_deref().unwrap_or_default())
    }

    /// The GPU measured: chosen on first want, then followed while it is
    /// there. A GPU missed is looked for again by its device, as a driver
    /// update gives it a new LUID; with none, it fails, and another is not
    /// measured in its place. None when there is no GPU.
    fn device(&mut self, file: &Log) -> Result<Option<&Device>, String> {
        if matches!(self.gpu, Gpu::Unchosen) {
            self.gpu = match adapters().map(|all| choose(&all).cloned()) {
                Ok(Some(device)) => {
                    log(file, format!("pc: {}", device.told()));
                    Gpu::Chosen(device, None)
                }
                Ok(None) => {
                    log(file, "pc: no GPU found; GPU and VRAM show --");
                    Gpu::Missing
                }
                Err(why) => return Err(format!("the GPUs could not be listed: {why}")),
            };
        }
        let luid = match &self.gpu {
            Gpu::Chosen(device, _) => device.luid.clone(),
            _ => return Ok(None),
        };
        let there = in_use(self.memory()?, &luid).is_some();
        let Gpu::Chosen(device, missed) = &mut self.gpu else {
            return Ok(None);
        };
        if there {
            *missed = None;
            return Ok(Some(&*device));
        }
        let now = Instant::now();
        if missed.is_none_or(|at| now.duration_since(at) >= SEEK) {
            *missed = Some(now);
            if let Some(again) = adapters()
                .unwrap_or_default()
                .into_iter()
                .find(|a| a.same(device))
            {
                log(
                    file,
                    format!("pc: {} found again as {}", device.name, again.luid),
                );
                *device = again;
                return Ok(Some(&*device));
            }
        }
        Err(format!("the GPU {} is not there", device.name))
    }
}

/// One counter in a query of its own, closed when dropped.
struct Counter {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

impl Counter {
    fn open(path: PCWSTR) -> Result<Self, String> {
        let mut query = PDH_HQUERY::default();
        checked(unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) })?;
        let mut counter = PDH_HCOUNTER::default();
        let added = checked(unsafe { PdhAddEnglishCounterW(query, path, 0, &mut counter) });
        let opened = Self { query, counter };
        added.map(|()| opened)
    }

    /// A rate's counter, collected: None just after it opens, which is its
    /// first sample, and after a gap, which starts it again.
    fn rate(
        held: &mut Option<Counter>,
        path: PCWSTR,
        steady: bool,
    ) -> Result<Option<&Counter>, String> {
        if held.is_none() {
            let counter = Counter::open(path)?;
            counter.collect()?;
            *held = Some(counter);
            return Ok(None);
        }
        let counter = held.as_ref().ok_or("the counter is not open")?;
        counter.collect()?;
        Ok(steady.then_some(counter))
    }

    fn collect(&self) -> Result<(), String> {
        checked(unsafe { PdhCollectQueryData(self.query) })
    }

    /// Its value, or None while it has none yet; the status PDH gave when
    /// it could not work one out.
    fn value(&self) -> Result<Option<f64>, u32> {
        let mut value = PDH_FMT_COUNTERVALUE::default();
        let status =
            unsafe { PdhGetFormattedCounterValue(self.counter, PDH_FMT_DOUBLE, None, &mut value) };
        if status == PDH_INVALID_DATA {
            return Ok(None);
        }
        if status != 0 {
            return Err(status);
        }
        Ok(valid(value.CStatus).then(|| unsafe { value.Anonymous.doubleValue }))
    }

    /// Each instance's value with its name in lower case, those with none
    /// yet left out.
    fn items(&self) -> Result<Vec<(String, f64)>, String> {
        let (mut size, mut count) = (0u32, 0u32);
        for _ in 0..3 {
            let status = unsafe {
                PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &mut size,
                    &mut count,
                    None,
                )
            };
            if status == PDH_INVALID_DATA || (status == 0 && count == 0) {
                return Ok(Vec::new());
            }
            if status != PDH_MORE_DATA {
                return Err(hex(status));
            }
            // Words, so the items' values are aligned as they must be.
            let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
            let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            let status = unsafe {
                PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &mut size,
                    &mut count,
                    Some(items),
                )
            };
            if status == PDH_MORE_DATA {
                continue;
            }
            checked(status)?;
            let items = unsafe { std::slice::from_raw_parts(items, count as usize) };
            return Ok(items
                .iter()
                .filter(|item| valid(item.FmtValue.CStatus))
                .filter_map(|item| {
                    let name = unsafe { item.szName.to_string() }.ok()?;
                    Some((name.to_ascii_lowercase(), unsafe {
                        item.FmtValue.Anonymous.doubleValue
                    }))
                })
                .collect());
        }
        Err(hex(PDH_MORE_DATA))
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.query) };
    }
}

fn valid(status: u32) -> bool {
    matches!(status, PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA)
}

/// Whether a sample's math came out below zero, as it does once each time
/// the 32-bit base of `% Processor Utility` rolls over: a sample to skip,
/// not a failure.
fn rolled_over(status: u32) -> bool {
    matches!(
        status,
        PDH_CALC_NEGATIVE_DENOMINATOR | PDH_CALC_NEGATIVE_TIMEBASE | PDH_CALC_NEGATIVE_VALUE
    )
}

/// The CPU's last share and how many samples in a row rolled over, so that
/// one that rolled over shows the share before it.
#[derive(Debug, Default)]
struct Held {
    last: Option<u8>,
    rolled: u32,
}

impl Held {
    /// A sample's share and, when it rolled over, the status that says so,
    /// with the last share in its place; more than MOST_ROLLED in a row, and
    /// any other status, fail.
    fn take(
        &mut self,
        value: Result<Option<f64>, u32>,
    ) -> Result<(Option<u8>, Option<u32>), String> {
        match value {
            Err(status) if rolled_over(status) => {
                self.rolled += 1;
                if self.rolled > MOST_ROLLED {
                    return Err(hex(status));
                }
                Ok((self.last, Some(status)))
            }
            Err(status) => {
                *self = Held::default();
                Err(hex(status))
            }
            Ok(value) => {
                self.rolled = 0;
                let share = value.map(percent);
                self.last = share.or(self.last);
                Ok((share, None))
            }
        }
    }
}

fn checked(status: u32) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(hex(status))
    }
}

fn hex(status: u32) -> String {
    format!("{status:#010x}")
}

/// A share as shown: rounded, and never past 100.
fn percent(share: f64) -> u8 {
    share.round().clamp(0.0, 100.0) as u8
}

/// The physical memory in use, as Task Manager counts it: all of it but
/// what is available.
fn ram() -> Result<Measured, String> {
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe { GlobalMemoryStatusEx(&mut status) }.map_err(|e| format!("{:#x}", e.code().0))?;
    if status.ullTotalPhys == 0 {
        return Err("no memory told".into());
    }
    let used = status.ullTotalPhys.saturating_sub(status.ullAvailPhys);
    Ok((
        Some(percent(used as f64 / status.ullTotalPhys as f64 * 100.0)),
        Some(Amount {
            used,
            total: status.ullTotalPhys,
            shared: false,
        }),
    ))
}

/// A GPU as DXGI lists it: what it is, by its device, its name, its memory
/// of its own and the computer's it may share, and its LUID as the counters
/// name it.
#[derive(Clone, Debug, PartialEq)]
struct Device {
    vendor: u32,
    device: u32,
    subsystem: u32,
    name: String,
    memory: u64,
    shared: u64,
    luid: String,
}

impl Device {
    fn same(&self, other: &Device) -> bool {
        (self.vendor, self.device, self.subsystem) == (other.vendor, other.device, other.subsystem)
    }
    /// Whether it is built in, by its memory of its own.
    fn built_in(&self) -> bool {
        self.memory < OWN_MEMORY
    }
    fn told(&self) -> String {
        let built_in = if self.built_in() {
            format!(
                "; built in, so VRAM is its shared memory ({:.1} GB)",
                self.shared as f64 / (1u64 << 30) as f64
            )
        } else {
            String::new()
        };
        format!(
            "GPU {} ({}), {} MB of its own{built_in}",
            self.name,
            self.luid,
            self.memory >> 20
        )
    }
}

/// The GPUs, not counting those drawn in software, in DXGI's order, which
/// lists the one with the main display first. Only their descriptions are
/// read; no device is made. Listing them takes the same turn as making a
/// device does: a security product's hook can sit under the graphics stack.
fn adapters() -> Result<Vec<Device>, String> {
    crate::icons::one_device_at_a_time(|| unsafe {
        let factory: IDXGIFactory1 =
            CreateDXGIFactory1().map_err(|e| format!("{:#x}", e.code().0))?;
        let mut all = Vec::new();
        for index in 0.. {
            let adapter = match factory.EnumAdapters1(index) {
                Ok(adapter) => adapter,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(format!("{:#x}", error.code().0)),
            };
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let length = desc
                .Description
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(desc.Description.len());
            all.push(Device {
                vendor: desc.VendorId,
                device: desc.DeviceId,
                subsystem: desc.SubSysId,
                name: String::from_utf16_lossy(&desc.Description[..length]),
                memory: desc.DedicatedVideoMemory as u64,
                shared: desc.SharedSystemMemory as u64,
                luid: luid(desc.AdapterLuid.HighPart, desc.AdapterLuid.LowPart),
            });
        }
        Ok(all)
    })
}

/// A LUID as the GPU counters write it in their instance names, in lower
/// case.
fn luid(high: i32, low: u32) -> String {
    format!("luid_0x{:08x}_0x{low:08x}", high as u32)
}

/// The GPU with the most memory of its own; the first listed among equals.
fn choose(all: &[Device]) -> Option<&Device> {
    all.iter().rev().max_by_key(|d| d.memory)
}

/// The busiest engine of the GPU with this LUID, each engine's share summed
/// over the processes using it, as Task Manager counts a GPU; None with no
/// engine listed for it. Instances are named like
/// `pid_1234_luid_0x00000000_0x0000f584_phys_0_eng_3_engtype_3d`.
fn busiest(items: &[(String, f64)], luid: &str) -> Option<f64> {
    let mut engines: HashMap<&str, f64> = HashMap::new();
    for (name, value) in items {
        if let Some(engine) = engine(name, luid) {
            *engines.entry(engine).or_default() += value;
        }
    }
    engines.into_values().reduce(f64::max)
}

/// The part of an engine's instance name that names the engine on its GPU,
/// from the LUID to its number, when it is one of this GPU's.
fn engine<'a>(name: &'a str, luid: &str) -> Option<&'a str> {
    let rest = &name[name.find(luid)?..];
    let at = rest.find("_eng_")? + "_eng_".len();
    let digits = rest[at..].bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0).then(|| &rest[..at + digits])
}

/// The memory of its own the GPU with this LUID uses, in bytes; None when
/// the counters do not list it.
fn in_use(items: &[(String, f64)], luid: &str) -> Option<f64> {
    items
        .iter()
        .filter(|(name, _)| name.starts_with(luid))
        .map(|(_, value)| *value)
        .reduce(|a, b| a + b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(memory: u64, low: u32) -> Device {
        Device {
            vendor: 0x8086,
            device: low,
            subsystem: 0,
            name: format!("GPU {low}"),
            memory,
            shared: 8 << 30,
            luid: luid(0, low),
        }
    }

    #[test]
    fn a_gpu_counts_as_its_busiest_engine() {
        // Each engine sums its processes; the GPU is the busiest engine, not
        // the sum or the mean of them, and another GPU's engines do not
        // count.
        let at = luid(0, 0xF584);
        assert_eq!(at, "luid_0x00000000_0x0000f584");
        let item = |pid: u32, low: u32, eng: u32, value: f64| {
            (
                format!("pid_{pid}_luid_0x00000000_0x{low:08x}_phys_0_eng_{eng}_engtype_3d"),
                value,
            )
        };
        let items = [
            item(10, 0xF584, 0, 20.0),
            item(11, 0xF584, 0, 15.0),
            item(10, 0xF584, 1, 30.0),
            item(12, 0xF584, 12, 3.0),
            item(10, 0xF974, 0, 90.0),
        ];
        assert_eq!(busiest(&items, &at), Some(35.0));
        assert_eq!(busiest(&items, &luid(0, 0x1234)), None);
        // An engine's number is all of its digits.
        assert_eq!(
            engine(&items[3].0, &at),
            Some("luid_0x00000000_0x0000f584_phys_0_eng_12")
        );
        // A busy GPU's processes may sum past 100.
        assert_eq!(percent(135.4), 100);
        assert_eq!(percent(34.5), 35);
        assert_eq!(percent(-1.0), 0);
    }

    #[test]
    fn a_cpu_sample_that_rolled_over_keeps_the_last_share() {
        // A sample whose math came out below zero, as when the counter's
        // 32-bit base rolls over, shows the share before it, or none yet;
        // more than five in a row fail, as any other status does, which also
        // lets the last share go.
        let negative = PDH_CALC_NEGATIVE_DENOMINATOR;
        let mut held = Held::default();
        assert_eq!(held.take(Err(negative)), Ok((None, Some(negative))));
        assert_eq!(held.take(Ok(Some(37.4))), Ok((Some(37), None)));
        for status in [
            PDH_CALC_NEGATIVE_DENOMINATOR,
            PDH_CALC_NEGATIVE_TIMEBASE,
            PDH_CALC_NEGATIVE_VALUE,
        ] {
            assert_eq!(held.take(Err(status)), Ok((Some(37), Some(status))));
        }
        assert_eq!(held.take(Ok(Some(40.0))), Ok((Some(40), None)));
        for _ in 0..MOST_ROLLED {
            assert_eq!(held.take(Err(negative)), Ok((Some(40), Some(negative))));
        }
        assert_eq!(held.take(Err(negative)), Err("0x800007d6".into()));
        assert_eq!(held.take(Err(negative)), Err("0x800007d6".into()));
        assert_eq!(held.take(Ok(Some(5.0))), Ok((Some(5), None)));
        // No value yet keeps the last share for the next roll.
        assert_eq!(held.take(Ok(None)), Ok((None, None)));
        assert_eq!(held.take(Err(negative)), Ok((Some(5), Some(negative))));
        assert_eq!(
            held.take(Err(PDH_INVALID_HANDLE)),
            Err(hex(PDH_INVALID_HANDLE))
        );
        assert_eq!(held.take(Err(negative)), Ok((None, Some(negative))));
        assert!(!rolled_over(PDH_INVALID_DATA));
    }

    #[test]
    fn vram_is_the_memory_of_the_gpu_s_own() {
        let items = [
            ("luid_0x00000000_0x0000f584_phys_0".to_string(), 2.0e9),
            ("luid_0x00000000_0x0000f974_phys_0".to_string(), 8.0e3),
        ];
        assert_eq!(in_use(&items, &luid(0, 0xF584)), Some(2.0e9));
        assert_eq!(in_use(&items, &luid(0, 0x1234)), None);
        // The LUID's high part is written as the bits it has.
        assert_eq!(luid(-1, 1), "luid_0xffffffff_0x00000001");
    }

    #[test]
    fn the_gpu_with_most_memory_of_its_own_is_chosen() {
        // A graphics card before the one built in; the first listed among
        // equals, which DXGI gives as the one with the main display.
        let built_in = device(128 << 20, 1);
        let card = device(8 << 30, 2);
        assert_eq!(choose(&[built_in.clone(), card.clone()]), Some(&card));
        let other = device(128 << 20, 3);
        assert_eq!(choose(&[built_in.clone(), other.clone()]), Some(&built_in));
        assert_eq!(choose(&[]), None);
        // The one built in tells that its VRAM is the memory it shares, and
        // how much that may be.
        assert!(built_in.built_in() && !card.built_in());
        assert!(
            built_in
                .told()
                .ends_with("; built in, so VRAM is its shared memory (8.0 GB)")
        );
        assert!(!card.told().contains("VRAM"));
        // Found again by its device, whatever its LUID.
        let moved = Device {
            luid: luid(0, 99),
            ..card.clone()
        };
        assert!(moved.same(&card) && !moved.same(&built_in));
    }

    #[test]
    fn the_log_says_what_is_measured() {
        assert_eq!(
            told([true, true, false, false]),
            "pc: measuring CPU RAM each second"
        );
        assert_eq!(told([false; 4]), "pc: not shown; not measured until it is");
        let shared = Amount {
            used: 1 << 30,
            total: 8 << 30,
            shared: true,
        };
        let pc = service(
            [
                Reading::Read(5),
                Reading::Pending,
                Reading::Failed(None),
                Reading::Read(100),
            ],
            [None, None, None, Some(shared)],
        );
        let labels: Vec<_> = pc
            .limits
            .iter()
            .flatten()
            .map(|l| l.label.as_str())
            .collect();
        assert_eq!(labels, METERS);
        assert!(pc.limits.iter().flatten().all(|l| l.resets.is_none()));
        assert_eq!(pc.meaning, Meaning::Meter);
        // Each meter carries its amount.
        let amounts: Vec<_> = pc.limits.iter().flatten().map(|l| l.amount).collect();
        assert_eq!(amounts, [None, None, None, Some(shared)]);
    }

    #[test]
    #[ignore = "measures this computer"]
    fn this_computer_is_measured() {
        let mut meters = Meters::default();
        let file: Log = Default::default();
        for step in 0..4 {
            let (readings, amounts) = meters.sample([true; 4], &file);
            println!("step {step}: {readings:?} {amounts:?}");
            std::thread::sleep(EVERY);
        }
        println!("adapters: {:?}", adapters());
        let engines = meters.engines.as_ref().unwrap().items().unwrap();
        println!("engine instances: {}", engines.len());
        assert!(matches!(
            meters.sample([true; 4], &file).0[RAM],
            Reading::Read(_)
        ));
        // With none shown, every counter closes, once.
        assert!(meters.close());
        assert!(!meters.close());
    }

    #[test]
    #[ignore = "watches this computer's GPU engines for 20 s"]
    fn engines_of_processes_started_later_are_counted() {
        let pids = |items: &[(String, f64)]| {
            items
                .iter()
                .filter_map(|(name, _)| name.split('_').nth(1)?.parse::<u32>().ok())
                .collect::<std::collections::BTreeSet<_>>()
        };
        let engines = Counter::open(ENGINES_COUNTER).unwrap();
        engines.collect().unwrap();
        std::thread::sleep(EVERY);
        engines.collect().unwrap();
        let first = pids(&engines.items().unwrap());
        for second in 1..=20 {
            std::thread::sleep(EVERY);
            engines.collect().unwrap();
            let now = pids(&engines.items().unwrap());
            let came: Vec<_> = now.difference(&first).collect();
            let went: Vec<_> = first.difference(&now).collect();
            println!(
                "{second:2} s: {} processes, came {came:?}, went {went:?}",
                now.len()
            );
        }
    }
}
