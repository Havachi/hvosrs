#![no_std]
#![no_main]
use uhv::*;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    println!("this is ls");
    exit(0);
    loop {}
}
