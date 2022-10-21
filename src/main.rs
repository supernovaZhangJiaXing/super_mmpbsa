mod index_parser;
mod mmpbsa;

use std::fs;
use std::env;
use std::io::{Read, stdin, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use regex::Regex;
use toml;
use toml::Value;

struct Parameters {
    rad_type:i32,
    rad_lj0:f64,
    mesh_type:i32,
    grid_type:i32,
    cfac:f64,
    fadd:f64,
    df:f64,
    dt:i32,
    nkernels:i32,
    preserve:bool,
    gmx:String,
    apbs:String
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut tpr = String::new();
    let mut trj = String::from("");
    let mut ndx = String::from("");
    let use_dh = true;
    let use_ts = true;

    // start workflow
    welcome();
    // initialize parameters
    let params = init_params();
    if !check_programs_validity(params.gmx.as_str(), params.apbs.as_str()) {
        println!("Error: Programs not correctly configured.");
        return;
    }
    match args.len() {
        1 => {
            println!("Input path of .tpr file, e.g. D:/Study/ZhangYang.tpr");
            stdin().read_line(&mut tpr).expect("Failed to read tpr file.");
        },
        2 => tpr = args[1].to_string(),
        _ => {
            for i in 1..args.len() {
                match args[i].as_str() {
                    "-f" => { trj = args[i + 1].to_string() },
                    "-s" => { tpr = args[i + 1].to_string() },
                    "-n" => { ndx = args[i + 1].to_string() },
                    _ => {
                        if i % 2 == 1 {
                            println!("Omitted invalid option: {}", args[i])
                        }
                    }
                }
            }
        },
    }
    tpr = confirm_file_validity(&mut tpr, vec!["tpr"]);
    // working directory (path of tpr location)
    let wd = Path::new(&tpr).parent().expect("Failed getting parent directory.");
    println!("Currently working at path: {}", wd.display());
    dump_tpr(&tpr, wd, params.gmx.as_str());
    loop {
        println!("\n                 ************ SuperMMPBSA functions ************");
        println!("-2 Toggle whether to use entropy contribution, current: {}", use_ts);
        println!("-1 Toggle whether to use Debye-Huckel shielding method, current: {}", use_dh);
        println!(" 0 Ready for MM-PBSA calculations");
        println!(" 1 Assign trajectory file (xtc or trr), current: {}", match trj.len() {
            0 => "undefined",
            _ => trj.as_str()
        });
        println!(" 2 Assign index file (ndx), current: {}", match ndx.len() {
            0 => "undefined",
            _ => ndx.as_str()
        });
        println!(" 3 Exit program");
        let i = get_input_sel();
        match i {
            -2 => { let use_dh = !use_dh; },
            -1 => { let use_ts = !use_ts; },
            0 => {
                if trj.len() == 0 {
                    println!("Trajectory file not assigned.");
                } else if ndx.len() == 0 {
                    // 可能要改, 以后不需要index也能算
                    println!("Index file not assigned.");
                } else {
                    mmpbsa_calculation(&trj, &tpr, &ndx, use_dh, use_ts);
                }
            },
            1 => {
                println!("Input trajectory file path:");
                stdin().read_line(&mut trj).expect("Failed while reading trajectory");
                trj = confirm_file_validity(&mut trj, vec!["xtc", "trr"]);
            },
            2 => {
                println!("Input index file path:");
                stdin().read_line(&mut ndx).expect("Failed while reading index");
                ndx = confirm_file_validity(&mut ndx, vec!["ndx"]);
            },
            3 => break,
            _ => println!("Error input.")
        };
    }
}

fn welcome() {
    println!("SuperMMPBSA: Supernova's tool of calculating binding free energy using\n\
molecular mechanics Poisson–Boltzmann surface area (MM-PBSA) method.\n\
Website: https://github.com/supernovaZhangJiaXing/super_mmpbsa\n\
Developed by Jiaxing Zhang (zhangjiaxing7137@tju.edu.cn), Tian Jin University.\n\
Version 0.1, first release: 2022-Oct-17\n\n\
Usage 1: run `SuperMMPBSA` and follow the prompts.\n\
Usage 2: run `SuperMMPBSA WangBingBing.tpr` to directly load WangBingBing.tpr.\n\
Usage 3: run `SuperMMPBSA -f md.xtc -s md.tpr -n index.ndx` to assign all needed files.\n");
}

fn dump_tpr(tpr:&String, wd:&Path, gmx:&str) {
    let tpr_dump = Command::new(gmx).arg("dump").arg("-s").arg(tpr).output().expect("gmx dump failed.");
    let tpr_dump = String::from_utf8(tpr_dump.stdout).expect("Getting dump output failed.");
    let mut outfile = fs::File::create(wd.join("_mdout.mdp")).unwrap();
    outfile.write(tpr_dump.as_bytes()).unwrap();
    println!("Finished loading tpr file, md parameters dumped to {}", wd.join("_mdout.mdp").display());
}

fn mmpbsa_calculation(trj:&String, tpr:&String, ndx:&String, use_dh:bool, use_ts:bool) {
    let mut complex_grp: i32 = -1;
    let mut receptor_grp: i32 = -1;
    let mut ligand_grp: i32 = -1;
    let ndx = index_parser::Index::new(ndx);
    loop {
        println!("\n                 ************ MM-PBSA calculation ************");
        println!("-10 Return");
        println!("  0 Do MM-PBSA calculations now!");
        println!("  1 Select complex group, current: {}", match complex_grp {
            -1 => String::from("undefined"),
            _ => format!("{} {}", complex_grp, ndx.groups[complex_grp as usize].name)
        });
        println!("  2 Select receptor groups, current: {}", match receptor_grp {
            -1 => String::from("undefined"),
            _ => format!("{} {}", receptor_grp, ndx.groups[receptor_grp as usize].name)
        });
        println!("  3 Select ligand groups, current: {}", match ligand_grp {
            -1 => String::from("undefined"),
            _ => format!("{} {}", ligand_grp, ndx.groups[ligand_grp as usize].name)
        });
        let i = get_input_sel();
        match i {
            -10 => return,
            0 => {
                mmpbsa::do_mmpbsa_calculations(trj, tpr, &ndx, use_dh, use_ts,
                                               complex_grp as usize,
                                               receptor_grp as usize,
                                               ligand_grp as usize);
                break;
            },
            1 => {
                ndx.list_groups();
                println!("Input complex group num:");
                complex_grp = get_input_sel();
            }
            2 => {
                ndx.list_groups();
                println!("Input receptor group num:");
                receptor_grp = get_input_sel();
            }
            3 => {
                ndx.list_groups();
                println!("Input ligand group num:");
                ligand_grp = get_input_sel();
            }
            _ => println!("Error input")
        }
    }
}

fn get_input_sel() -> i32 {
    let mut input = String::from("");
    stdin().read_line(&mut input).expect("Error input.");
    while input.trim().len() == 0 {
        stdin().read_line(&mut input).expect("Error input.");
    }
    let temp: i32 = input.trim().parse().expect("Error convert to int.");
    return temp;
}

fn confirm_file_validity(file_name: &mut String, ext_list: Vec<&str>) -> String {
    let mut file_path = Path::new(file_name.trim());
    loop {
        // check validity
        if !file_path.is_file() {
            println!("Not valid file: {}. Input file path again.", file_path.display());
            file_name.clear();
            stdin().read_line(file_name).expect("Failed to read file name.");
            file_path = Path::new(file_name.trim());
            continue;
        }
        // check extension
        let file_ext = Path::new(file_path).extension().unwrap().to_str().unwrap();
        for i in 0..ext_list.len() {
            if file_ext != ext_list[i] {
                continue;
            } else {
                return file_name.trim().to_string();
            }
        }
        println!("Not valid {:?} file, currently {}. Input file path again.", ext_list, file_ext);
        file_name.clear();
        stdin().read_line(file_name).expect("Failed to read file name.");
        file_path = Path::new(file_name.trim());
    }
}

fn check_programs_validity(gmx: &str, apbs: &str) -> bool {
    // gmx
    let test_gmx = Command::new(gmx).arg("--version").output().expect("running gmx failed.");
    let test_gmx = String::from_utf8(test_gmx.stdout).expect("Getting gmx output failed.");
    if test_gmx.find("GROMACS version") == None {
        println!("GROMACS status: ERROR");
        return false;
    }

    // apbs
    let test_apbs = Command::new(apbs).arg("--version").output().expect("running apbs failed.");
    let test_apbs = String::from_utf8(test_apbs.stdout).expect("Getting apbs output failed.");
    if test_apbs.find("Version") == None {
        println!("APBS status: ERROR");
        return false;
    }
    fs::remove_file(Path::new("io.mc")).unwrap();
    return true;
}

fn init_params() -> Parameters {
    let mut params: Parameters;
    if Path::new("settings.ini").is_file() {
        // Find settings locally
        let settings = fs::read_to_string("settings.ini").unwrap();
        let settings = Regex::new(r"\\").unwrap().replace_all(settings.as_str(), "/").to_string();
        let settings: Value = toml::from_str(settings.as_str()).unwrap();
        params = read_settings(&settings);
        println!("Found settings.ini in the current path. Will use {} kernels.", params.nkernels);
    } else {
        // Find $SuperMMPBSAPath
        let mut super_mmpbsa_path = String::from("");
        // to help determine if file exists in $SuperMMPBSAPath
        let mut path: PathBuf = PathBuf::new();
        for (k, v) in env::vars() {
            if k == "SuperMMPBSAPath" {
                super_mmpbsa_path = v;
            }
        }
        if super_mmpbsa_path != "" {
            for entry in Path::new(super_mmpbsa_path.as_str()).read_dir().unwrap() {
                let entry = entry.unwrap();
                path = entry.path();
                let fname = path.file_name().unwrap();
                if fname == "settings.ini" {
                    break;
                }
            }
        }
        if super_mmpbsa_path != "" && path.file_name().unwrap() == "settings.ini" {
            let settings = fs::read_to_string(path).unwrap();
            let settings = Regex::new(r"\\").unwrap()
                .replace_all(settings.as_str(), "/").to_string();
            let settings: Value = toml::from_str(settings.as_str()).unwrap();
            params = read_settings(&settings);
            println!("Found settings.ini in $SuperMMPBSAPath. Will use {} kernels.", params.nkernels);
        } else {
            params = Parameters { rad_type: 1, rad_lj0: 1.2, mesh_type: 0, grid_type: 1, cfac: 3.0,
                fadd: 10.0, df: 0.5, dt: 1000, nkernels: 1, preserve: true,
                gmx: String::from("gmx"), apbs: String::from("apbs") };
            println!("Did not found settings.ini. Will use 1 kernels.");
        }
    }
    return params;
}

fn read_settings(settings: &Value) -> Parameters {
    let rad_type = settings.get("radType").unwrap().to_string().parse().unwrap();
    let rad_lj0 = settings.get("radLJ0").unwrap().to_string().parse().unwrap();
    let mesh_type = settings.get("meshType").unwrap().to_string().parse().unwrap();
    let grid_type = settings.get("gridType").unwrap().to_string().parse().unwrap();
    let cfac = settings.get("cfac").unwrap().to_string().parse().unwrap();
    let fadd = settings.get("fadd").unwrap().to_string().parse().unwrap();
    let df = settings.get("df").unwrap().to_string().parse().unwrap();
    let dt = settings.get("dt").unwrap().to_string().parse().unwrap();
    let nkernels = settings.get("nkernels").unwrap().to_string().parse().unwrap();
    let preserve = match settings.get("preserve").unwrap().as_str().unwrap() {
        "y" => true,
        "Y" => true,
        _ => false
    };
    let mut gmx = settings.get("gmx").unwrap().to_string();
    if gmx.starts_with("\"") && gmx.ends_with("\"") {
        gmx = gmx[1..gmx.len() - 1].to_string();
    }
    let mut apbs = settings.get("apbs").unwrap().to_string();
    if apbs.starts_with("\"") && apbs.ends_with("\"") {
        apbs = apbs[1..apbs.len() - 1].to_string();
    }
    return Parameters { rad_type, rad_lj0, mesh_type, grid_type, cfac, fadd,
        df, dt, nkernels, preserve, gmx, apbs };
}