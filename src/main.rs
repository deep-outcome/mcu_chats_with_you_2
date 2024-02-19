#![no_std]
#![no_main]

#[cfg(feature = "panic_halt")]
use panic_halt as _;

use core::cell::{Cell, OnceCell, RefCell};
use cortex_m::interrupt::free as interrupt_free;
use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use microbit::hal::Rng;
use microbit::{
    display::nonblocking::{Display, GreyscaleImage},
    hal::rtc::{Rtc, RtcInterrupt},
    pac::{interrupt, RTC0, TIMER2},
};

static DISPLAYOR: Mutex<RefCell<Option<Display<TIMER2>>>> = Mutex::new(RefCell::new(None));
static ANIMATOR: Mutex<OnceCell<Rtc<RTC0>>> = Mutex::new(OnceCell::new());
static RND: Mutex<Cell<Option<Rng>>> = Mutex::new(Cell::new(None));

#[entry]
fn entry() -> ! {
    use microbit::board::Board;
    use microbit::pac::{Interrupt, NVIC};

    let mut board = Board::take().unwrap();

    microbit::hal::clocks::Clocks::new(board.CLOCK).start_lfclk();
    let mut rtc0 = Rtc::new(board.RTC0, 327).unwrap();
    rtc0.enable_interrupt(RtcInterrupt::Tick, None);
    rtc0.enable_counter();

    let display = Display::new(board.TIMER2, board.display_pins);

    let rnd = Rng::new(board.RNG);

    interrupt_free(move |cs| {
        DISPLAYOR.borrow(cs).replace(Some(display));
        _ = ANIMATOR.borrow(cs).set(rtc0);
        RND.borrow(cs).set(Some(rnd));
    });

    unsafe {
        board.NVIC.set_priority(Interrupt::RTC0, 64);
        board.NVIC.set_priority(Interrupt::TIMER2, 32);

        NVIC::unmask(Interrupt::RTC0);
        NVIC::unmask(Interrupt::TIMER2);
    }

    loop {}
}

#[interrupt]
fn TIMER2() {
    interrupt_free(|cs| {
        let borrow = DISPLAYOR.borrow(cs);
        let mut refmut = borrow.borrow_mut();
        refmut.as_mut().unwrap().handle_display_event();
    });
}

#[interrupt]
unsafe fn RTC0() {
    use core::sync::atomic::{AtomicU8, Ordering};

    interrupt_free(|cs| {
        let animator = ANIMATOR.borrow(cs).get().unwrap();
        animator.reset_event(RtcInterrupt::Tick);
    });

    static mut COL_DEF_IX: usize = 0;
    static mut COL_IX: usize = 0;

    static mut DISP_LATT: [[u8; 5]; 5] = [
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
    ];

    static mut SCALER: AtomicU8 = AtomicU8::new(0);
    static mut INS_SP: AtomicU8 = AtomicU8::new(0);

    const TEXT: &str = "software9119.technology";
    const TEXT_PTR: *const u8 = TEXT.as_ptr();

    if SCALER.fetch_add(1, Ordering::Relaxed) < 18 {
        return;
    } else {
        SCALER.swap(0, Ordering::Relaxed);
    }

    for cix in 1..5 {
        let prev_cix = cix - 1;
        for rix in 0..5 {
            DISP_LATT[rix][prev_cix] = DISP_LATT[rix][cix];
        }
    }

    let ins_sp = INS_SP.load(Ordering::Relaxed);

    let def = if ins_sp > 0 {
        &ug_max::SPACING
    } else {
        ug_max::col_def(TEXT_PTR.offset(COL_DEF_IX as isize).read() as char)
    };

    let col = def[COL_IX];

    let mut rnd = interrupt_free(|cs| {
        let borrow = RND.borrow(cs);
        borrow.take().unwrap()
    });

    for rix in 0..5 {
        let mask = 1 << rix;

        let brightness = if col & mask == mask {
            let rnd = rnd.random_u8() % 10;

            match rnd {
                0..=5 => 5,
                x => x,
            }
        } else {
            0
        };

        DISP_LATT[rix][4] = brightness;
    }

    let gsi = GreyscaleImage::new(&DISP_LATT);

    interrupt_free(|cs| {
        let rnd_borrow = RND.borrow(cs);
        rnd_borrow.set(Some(rnd));

        let dis_borrow = DISPLAYOR.borrow(cs);
        let mut refmut = dis_borrow.borrow_mut();
        refmut.as_mut().unwrap().show(&gsi);
    });

    COL_IX += 1;
    if COL_IX == def.len() {
        COL_IX = 0;

        let sp = if ins_sp == 0 {
            COL_DEF_IX += 1;

            if COL_DEF_IX == TEXT.len() {
                COL_DEF_IX = 0;
                5
            } else {
                1
            }
        } else {
            ins_sp - 1
        };

        INS_SP.store(sp, Ordering::Relaxed);
    }
}

#[cfg(feature = "panic_abort")]
mod panic_abort {
    use core::panic::PanicInfo;

    #[panic_handler]
    fn panic(_info: &PanicInfo) -> ! {
        loop {}
    }
}

// cargo flash --target thumbv7em-none-eabihf --chip nRF52833_xxAA --release --features panic_abort
// cargo flash --target thumbv7em-none-eabihf --chip nRF52833_xxAA --features panic_halt
// cargo build --release  --target thumbv7em-none-eabihf --features panic_abort
// cargo build --target thumbv7em-none-eabihf --features panic_halt
