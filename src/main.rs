use ovmf_prebuilt::{Arch, FileType, Prebuilt, Source};
use std::path::PathBuf;
use std::process::{Command, exit};
use clap::Parser;

#[derive(Debug, Clone)]
enum FirmwareMode {
    Uefi,
    Bios,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Boot using OVMF (UEFI)
    #[arg(long, conflicts_with = "bios")]
    uefi: bool,

    /// Boot using legacy BIOS
    #[arg(long, conflicts_with = "uefi")]
    bios: bool,

    /// Path to the elf image to boot
    #[arg(value_name = "elf_image", value_parser = clap::value_parser!(PathBuf))]
    elf_image: PathBuf,
}

fn build_boot_img(elf_image: &PathBuf, img_path: &PathBuf, firmware_mode: &FirmwareMode) {
    match firmware_mode {
        FirmwareMode::Uefi => {
            bootloader::UefiBoot::new(elf_image)
                .create_disk_image(img_path)
                .unwrap();
        }
        FirmwareMode::Bios => {
            bootloader::BiosBoot::new(elf_image)
                .create_disk_image(img_path)
                .unwrap();
        }
    }
}

fn start_qemu(img_path: &PathBuf, firmware_mode: &FirmwareMode) -> i32 {
    let mut cmd = Command::new("qemu-system-x86_64");
    // print serial output to the shell
    cmd.arg("-serial").arg("mon:stdio");
    // enable the guest to exit qemu
    cmd.arg("-device")
        .arg("isa-debug-exit,iobase=0xf4,iosize=0x04");

    let parent_dir = img_path.parent().unwrap_or_else(|| {
        eprintln!("Error: Could not determine the parent directory of the image.");
        exit(1);
    });
    let img_path = img_path.display();
    let ovmf_path = parent_dir.join("ovmf");

    match firmware_mode {
        FirmwareMode::Uefi => {
        let prebuilt =
            Prebuilt::fetch(Source::LATEST, ovmf_path).expect("failed to update prebuilt");

        let code = prebuilt.get_file(Arch::X64, FileType::Code);
        let vars = prebuilt.get_file(Arch::X64, FileType::Vars);

        cmd.arg("-drive")
            .arg(format!("format=raw,file={img_path}"));
        cmd.arg("-drive").arg(format!(
            "if=pflash,format=raw,unit=0,file={},readonly=on",
            code.display()
        ));
        // copy vars and enable rw instead of snapshot if you want to store data (e.g. enroll secure boot keys)
        cmd.arg("-drive").arg(format!(
            "if=pflash,format=raw,unit=1,file={},snapshot=on",
            vars.display()
        ));
    }
        FirmwareMode::Bios => {
            cmd.arg("-drive")
                .arg(format!("format=raw,file={img_path}"));
        }
    }

    let mut child = cmd.spawn().expect("failed to start qemu-system-x86_64");
    let status = child.wait().expect("failed to wait on qemu");
    println!("QEMU exited with status: {}", status);

    const SUCCESS: i32 = (0x10 << 1) | 1;
    const FAILURE: i32 = (0x11 << 1) | 1;

    match status.code().unwrap_or(1) {
        SUCCESS => 0, // success
        FAILURE => 1, // failure
        _ => 2,       // unknown fault
    }
}


fn main() {
    let args = Args::parse();

    let elf_image = args.elf_image;
    let is_uefi = args.uefi;
    let is_bios = args.bios;

    let firmware_mode = if is_uefi {
        FirmwareMode::Uefi
    } else if is_bios {
        FirmwareMode::Bios
    } else {
        eprintln!("Error: Please specify either --uefi or --bios.");
        exit(1);
    };

    let parent_dir = elf_image.parent().unwrap_or_else(|| {
        eprintln!("Error: Could not determine the parent directory of the ELF image.");
        exit(1);
    });

    let img_path = parent_dir.join("boot.img");

    build_boot_img(&elf_image, &img_path, &firmware_mode);

    let exit_code = start_qemu(&img_path, &firmware_mode);
    exit(exit_code);
}
