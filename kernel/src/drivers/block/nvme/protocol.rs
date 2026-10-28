use x86_64::VirtAddr;

#[repr(C, packed)]
pub struct NvmeCommand {
	pub opcode: u8,
	pub flags: u8,
	pub cid: u16,
	pub nsid: u32,
	pub rsvd2: u64,
	pub metaptr: u64,
	pub prp1: u64,
	pub prp2: u64,
	pub cdw10: u32,
	pub cdw11: u32,
	pub cdw12: u32,
	pub cdw13: u32,
	pub cdw14: u32,
	pub cdw15: u32,
}

impl NvmeCommand {
	pub fn zero() -> Self {
		Self {
			opcode: 0,
			flags: 0,
			cid: 0,
			nsid: 0,
			rsvd2: 0,
			metaptr: 0,
			prp1: 0,
			prp2: 0,
			cdw10: 0,
			cdw11: 0,
			cdw12: 0,
			cdw13: 0,
			cdw14: 0,
			cdw15: 0,
		}
	}

	pub fn new_identify(nsid: u32, nsc: u32, buf_phys: u64) -> Self {
		Self {
			opcode: 0x06,
			flags: 0,
			cid: 1,
			nsid,
			rsvd2: 0,
			metaptr: 0,
			prp1: buf_phys,
			prp2: 0,
			cdw10: nsc,
			cdw11: 0,
			cdw12: 0,
			cdw13: 0,
			cdw14: 0,
			cdw15: 0,
		}
	}
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct CompletionQueueEntry {
	pub dw0: u32,
	pub dw1: u32,
	pub sq_head_ptr: u16,
	pub sq_id: u16,
	pub command_id: u16,
	pub status: u16,
}

impl CompletionQueueEntry {
	pub fn phase_bit(self) -> bool {
		self.status & 0x1 != 0
	}

	pub fn status_code(self) -> u16 {
		self.status >> 1
	}
}

#[repr(C, packed)]
pub struct NvmeIdCtrl {
	pub vid: u16,
	pub ssvid: u16,
	pub sn: [u8; 20],
	pub mn: [u8; 40],
	pub fr: [u8; 8],
	pub rab: u8,
	pub ieee: [u8; 3],
	pub cmic: u8,
	pub mdts: u8,
	pub cntlid: u16,
	pub ver: u32,
	pub rtd3r: u32,
	pub rtd3e: u32,
	pub oaes: u32,
	pub ccs: u32,
	pub rsvd100: [u8; 12],
	pub oacs: u16,
	pub acl: u8,
	pub aerl: u8,
	pub fww: u8,
	pub lpa: u8,
	pub elpe: u8,
	pub npss: u8,
	pub avscc: u8,
	pub apsta: u8,
	pub wctemp: u16,
	pub cctemp: u16,
	pub mtfa: u16,
	pub hmpre: u32,
	pub hmmin: u32,
	pub tnvmcap: [u8; 16],
	pub unvmcap: [u8; 16],
	pub rpmbs: u32,
	pub edstt: u16,
	pub dsto: u8,
	pub fwug: u8,
	pub kas: u16,
	pub hctma: u16,
	pub mntmt: u16,
	pub mxtmt: u16,
	pub sanicap: u32,
	pub rsvd384: [u8; 68],
	pub sqes: u8,
	pub cqes: u8,
	pub maxcmd: u16,
	pub nn: u32,
	pub oncs: u16,
	pub fuses: u16,
	pub fna: u8,
	pub vwc: u8,
	pub awun: u16,
	pub awupf: u16,
	pub nvscc: u8,
	pub nwpc: u8,
	pub acwu: u16,
	pub rsvd534: [u8; 2],
	pub sgls: u32,
	pub mnan: u32,
	pub rsvd544: [u8; 224],
	pub subnqn: [u8; 256],
	pub rsvd1024: [u8; 768],
}

impl NvmeIdCtrl {
	pub fn from_virt(buf_addr: VirtAddr) -> Self {
		let ptr = buf_addr.as_ptr() as *const Self;
		unsafe { core::ptr::read_unaligned(ptr) }
	}
}

#[repr(C, packed)]
pub struct NvmeIdNs {
	pub nsze: u64,
	pub ncap: u64,
	pub nuse: u64,
	pub nsfeat: u8,
	pub nlbaf: u8,
	pub flbas: u8,
	pub mc: u8,
	pub dpc: u8,
	pub dps: u8,
	pub nmic: u8,
	pub rescap: u8,
	pub fpi: u8,
	pub dlfeat: u8,
	pub nawun: u16,
	pub nawupf: u16,
	pub nacwu: u16,
	pub nabsn: u16,
	pub nabo: u16,
	pub nabspf: u16,
	pub noiob: u16,
	pub nvmcap: [u8; 16],
	pub rsvd64: [u8; 40],
	pub lbaf: [u32; 16],
}

impl NvmeIdNs {
	pub fn from_virt(buf_addr: VirtAddr) -> Self {
		let ptr = buf_addr.as_ptr() as *const Self;
		unsafe { core::ptr::read_unaligned(ptr) }
	}
}
