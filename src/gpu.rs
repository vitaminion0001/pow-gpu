use ocl;
use ocl::ProQue;
use ocl::Result;
use ocl::Buffer;
use ocl::Platform;
use ocl::prm::Ulong;
use ocl::flags::MemFlags;
use ocl::builders::ProgramBuilder;
use ocl::builders::DeviceSpecifier;

use byteorder::{ByteOrder, LittleEndian};

pub struct Gpu {
    kernel: ocl::Kernel,
    attempt: Buffer<u8>,
    result: Buffer<u8>,
    root: Buffer<u8>,
    threshold: Buffer<u8>,
}

impl Gpu {
    pub fn new(platform_idx: usize, device_idx: usize, threads: usize) -> Result<Gpu> {
        let mut prog_bldr = ProgramBuilder::new();
        prog_bldr.src(include_str!("work.cl"));
        let platforms = Platform::list();
        if platforms.len() == 0 {
            return Err("No OpenCL platforms exist (check your drivers and OpenCL setup)".into());
        }
        if platform_idx >= platforms.len() {
            return Err(format!(
                "Platform index {} too large (max {})",
                platform_idx,
                platforms.len() - 1
            ).into());
        }
        let pro_que = ProQue::builder()
            .prog_bldr(prog_bldr)
            .platform(platforms[platform_idx])
            .device(DeviceSpecifier::Indices(vec![device_idx]))
            .dims(1)
            .build()?;

        let device = pro_que.device();
        eprintln!(
            "Initializing GPU: {} {}",
            device.vendor().unwrap_or_else(|_| "[unknown]".into()),
            device.name().unwrap_or_else(|_| "[unknown]".into())
        );

        let attempt = Buffer::<u8>::builder()
            .queue(pro_que.queue().clone())
            .flags(MemFlags::new().read_only().host_write_only())
            .len(8)
            .build()?;
        let result = Buffer::<u8>::builder()
            .queue(pro_que.queue().clone())
            .flags(MemFlags::new().write_only())
            .len(8)
            .build()?;
        let root = Buffer::<u8>::builder()
            .queue(pro_que.queue().clone())
            .flags(MemFlags::new().read_only().host_write_only())
            .len(32)
            .build()?;
        let threshold = Buffer::<u8>::builder()
            .queue(pro_que.queue().clone())
            .flags(MemFlags::new().read_only().host_write_only())
            .len(32)
            .build()?;

        let kernel = pro_que
            .kernel_builder("vitechain_work")
            .global_work_size(threads)
            .arg(&attempt)
            .arg(&result)
            .arg(&root)
            .arg(&threshold)
            .build()?;

        let mut gpu = Gpu {
            kernel,
            attempt,
            result,
            root,
            threshold,
        };
        gpu.reset_bufs()?;
        Ok(gpu)
    }

    pub fn reset_bufs(&mut self) -> Result<()> {
        self.result.write(&[0u8; 8] as &[u8]).enq()?;
        Ok(())
    }

    pub fn set_task(&mut self, root: &[u8], threshold: &[u8]) -> Result<()> {
        self.reset_bufs()?;
        self.root.write(root).enq()?;
        self.threshold.write(threshold).enq()?;
        Ok(())
    }

    pub fn try(&mut self, out: &mut [u8], attempt: u64) -> Result<bool> {
        let mut attempt_bytes = [0u8; 8];
        LittleEndian::write_u64(&mut attempt_bytes, attempt);
        self.attempt.write(&attempt_bytes as &[u8]).enq()?;
        debug_assert!(out.iter().all(|&b| b == 0));
        debug_assert!({
            let mut result = [0u8; 8];
            self.result.read(&mut result as &mut [u8]).enq()?;
            result.iter().all(|&b| b == 0)
        });

        unsafe {
            self.kernel.enq()?;
        }

        self.result.read(&mut *out).enq()?;
        let success = !out.iter().all(|&b| b == 0);
        if success {
            self.reset_bufs()?;
        }
        Ok(success)
    }
}