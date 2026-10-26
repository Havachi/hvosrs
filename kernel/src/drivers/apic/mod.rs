use crate::kwarn;



pub fn init() {
	if !apic_support() {
		kwarn!("Processor doesn't support APIC");
		return;
	}
}

fn apic_support() -> bool {
	let cpuid = raw_cpuid::CpuId::new();
	return if let Some(info) = cpuid.get_feature_info() { 
		info.has_apic()
	} else {
		false
	}
}