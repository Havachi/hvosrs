#![no_std]
#![no_main]

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

pub fn write(str: &str) {
    let len = str.len();
    let addr = str.as_ptr() as u64;

    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 1u64,
            in("rdi") 1u64,
            in("rsi") addr,
            in("rdx") len as u64
        )
    }

}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Userland program entry point
    write("Hello from init !");
    loop {
        unsafe {
            core::arch::asm!("pause");
        }
    }
}