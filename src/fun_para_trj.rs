use std::path::Path;
use crate::{get_input_selection, parameters::Parameters};
use crate::atom_radius::Radius;
use crate::fun_para_mmpbsa::set_para_mmpbsa;
use crate::index_parser::Index;
use crate::parse_tpr::TPR;

pub fn set_para_trj(trj: &String, tpr: &mut TPR, ndx: &String, wd: &Path, atom_radius: &Radius, settings: &mut Parameters) {
    let mut receptor_grp: Option<usize> = None;
    let mut ligand_grp: Option<usize> = None;
    let mut bt: f64 = 0.0;                                  // ps
    let mut et: f64 = tpr.dt * tpr.nsteps as f64;           // ps
    let mut dt: f64 = tpr.dt * tpr.nstxout as f64;          // ps
    let unit_dt: f64 = tpr.dt * tpr.nstxout as f64;         // ps
    let ndx = Index::new(ndx);
    loop {
        println!("\n                 ************ Trajectory Parameters ************");
        println!("-10 Return");
        println!("  0 Go to next step");
        println!("  1 Select receptor groups, current:          {}", show_grp(receptor_grp, &ndx));
        println!("  2 Select ligand groups, current:            {}", show_grp(ligand_grp, &ndx));
        println!("  3 Set start time of analysis, current:      {} ns", bt / 1000.0);
        println!("  4 Set end time of analysis, current:        {} ns", et / 1000.0);
        println!("  5 Set time interval of analysis, current:   {} ns", dt / 1000.0);
        let i = get_input_selection();
        match i {
            -10 => return,
            0 => {
                match receptor_grp {
                    Some(receptor_grp) => {
                        set_para_mmpbsa(trj, tpr, &ndx, wd,
                            receptor_grp,
                            ligand_grp,
                            bt, et, dt, atom_radius,
                            settings);
                    }
                    _ => println!("Please select receptor groups.")
                }
            }
            1 => {
                println!("Current groups:");
                ndx.list_groups();
                println!("Input receptor group num:");
                receptor_grp = Some(get_input_selection());
            }
            2 => {
                println!("Current groups:");
                ndx.list_groups();
                println!("Input ligand group num (-1 for nothing):");
                ligand_grp = match get_input_selection() {
                    -1 => None,
                    i => Some(i as usize)
                };
            }
            3 => {
                println!("Input start time (ns), should be divisible of {} ps:", dt);
                let mut new_bt = get_input_selection::<f64>() * 1000.0;
                while new_bt * 1000.0 % dt != 0.0 || new_bt > tpr.nsteps as f64 * tpr.dt as f64 || new_bt < 0.0 {
                    println!("The input {} ns not a valid time in trajectory.", new_bt / 1000.0);
                    println!("Input start time (ns) again, should be divisible of {} fs:", dt);
                    new_bt = get_input_selection::<f64>() * 1000.0;
                }
                bt = new_bt;
            }
            4 => {
                println!("Input end time (ns), should be divisible of {} ps:", dt);
                let mut new_et = get_input_selection::<f64>() * 1000.0;
                while new_et * 1000.0 % dt != 0.0 || new_et > tpr.nsteps as f64 * tpr.dt as f64 || new_et < 0.0 {
                    println!("The input {} ns not a valid time in trajectory.", new_et / 1000.0);
                    println!("Input end time (ns) again, should be divisible of {} fs:", dt);
                    new_et = get_input_selection::<f64>() * 1000.0;
                }
                et = new_et;
            }
            5 => {
                println!("Input interval time (ns), should be divisible of {} ps:", unit_dt);
                let mut new_dt = get_input_selection::<f64>() * 1000.0;
                while new_dt * 1000.0 % unit_dt != 0.0 {
                    println!("The input {} ns is not a valid time step.", new_dt / 1000.0);
                    println!("Input interval time (ns) again, should be divisible of {} ps:", unit_dt);
                    new_dt = get_input_selection::<f64>() * 1000.0;
                }
                dt = new_dt;
            }
            _ => println!("Invalid input")
        }
    }
}

fn show_grp(grp: Option<usize>, ndx: &Index) -> String {
    match grp {
        None => String::from("undefined"),
        Some(grp) => format!("{}): {}, {} atoms",
                    grp,
                    ndx.groups[grp as usize].name,
                    ndx.groups[grp as usize].indexes.len())
    }
}