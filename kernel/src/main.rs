#![no_std]
#![no_main]
use kernel::limine_requests::MODULES;
use kernel::scheduler::process;
use kernel::{drivers, interrupts, kerror, klog, scheduler};
use kernel::{kprint, memory};
use kernel::arch::x86_64::{
    cpu, gdt, idt, syscall
};

fn init() {
    x86_64::instructions::interrupts::disable();
    cpu::init();
    gdt::init();
    idt::init();
    memory::init();
    drivers::init();
    interrupts::init();
    scheduler::init();
    x86_64::instructions::interrupts::enable();
    syscall::init();

    if let Some(modules_response) = MODULES.response(){
        if let Some(init_module) = modules_response.modules().first() {
            if init_module.path().contains("init") {
                klog!("loading init");
                if let Ok((pid, proc)) = process::create_process_from_elf(init_module.data(), 0).map_err(|e| kerror!("Failed loading module: {e}")) {
                    scheduler::add_proc(pid, proc);
                    klog!("{} -> {}", init_module.path(), pid);
                }
            }
        }
    }
    klog!("kernel init done");
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() {
    init();
    //kprint!("\x1B[H\x1B[2J");

    kprint!("HvOS 0.1\n");
    scheduler::schedule();
    loop {
        //unsafe {core::arch::asm!("hlt")};
    }
}