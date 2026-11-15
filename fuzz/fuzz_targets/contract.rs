#![no_main]
#![forbid(unsafe_code)]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    use analyte_contract::*;
    let _ = decode::<Delivery>(data);
    let _ = decode::<Order>(data);
    let _ = decode::<OrderEvent>(data);
    let _ = decode::<OrdersPage>(data);
});
