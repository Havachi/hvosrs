#![no_std]
#![no_main]
use kernel::fs::cpio::load_initrd;
use kernel::limine_requests::MODULES;
use kernel::scheduler::process;
use kernel::vfs::file::VfsFileOpenflags;
use kernel::{cmos, drivers, interrupts, kerror, klog, scheduler, vfs};
use kernel::{kprint, memory};
use kernel::arch::x86_64::{
    cpu, gdt, idt, syscall
};

fn init() {
    x86_64::instructions::interrupts::disable();
    kernel::time::init();
    let pre = kernel::time::current_time();
    cpu::init();
    gdt::init();
    idt::init();    
    memory::init();
    drivers::init();
    interrupts::init();
    scheduler::init();

    vfs::init();
    x86_64::instructions::interrupts::enable();
    syscall::init();
    if let Some(modules_response) = MODULES.response(){

        for module in modules_response.modules() {
            if module.path() == "/initrd" {
                klog!("loading initrd");
                let _= load_initrd(module.data());
            }
        }
    }
    let post = kernel::time::current_time();
    kprint!("init completed in {}ms\n", post.to_unixtime().ms - pre.to_unixtime().ms);
    klog!("kernel init done");
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() {
    init();
    //kprint!("\x1B[H\x1B[2J");

    kprint!("HvOS 0.1\n");

    let mut init_file = vfs::open("/bin/init", VfsFileOpenflags::O_RDONLY.bits(), 0o777).unwrap();
    
    let data = vfs::read_file_all(&mut init_file).unwrap();
    if let Ok((pid, proc)) = process::create_process_from_elf(&data, 0)
        .map_err(|e| kerror!("Failed loading module: {e}")) 
    {
        scheduler::add_proc(pid, proc);
    }
    scheduler::schedule();
    loop {
        unsafe {core::arch::asm!("hlt")};
    }
}