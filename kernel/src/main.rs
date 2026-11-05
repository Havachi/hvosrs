#![no_std]
#![no_main]
use kernel::limine_requests::MODULES;
use kernel::scheduler::process;
use kernel::vfs::file::VfsFileOpenflags;
use kernel::{drivers, interrupts, kerror, klog, ksprint, scheduler, vfs};
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

    vfs::init();

    x86_64::instructions::interrupts::enable();
    syscall::init();

    if let Some(modules_response) = MODULES.response(){

        for module in modules_response.modules() {
            if module.path().contains("init") {
                klog!("loading init");
                if let Ok((pid, proc)) = process::create_process_from_elf(module.data(), 0).map_err(|e| kerror!("Failed loading module: {e}")) {
                    scheduler::add_proc(pid, proc);
                    klog!("{} -> {}", module.path(), pid);
                }
            }
            if module.path().contains("shell") {
                let _ = vfs::create("/bin/shell");
                let mut file = vfs::open("/bin/shell", VfsFileOpenflags::O_RDWR.bits()).expect("open /bin/shell");
                match vfs::write(&mut file, module.data()) {
                    Ok(written) => {
                        klog!("written {}B in {}", written, module.path());
                    },
                    Err(err) => {
                        kerror!("error while writting to /bin/shell: {:?}", err);
                    },
                };
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