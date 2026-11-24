use std::process::ExitCode;

use lokus_audiencias_lib::ingesta::{ejecutar_ingesta, parsear_argumentos_cli};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parsear_argumentos_cli(args) {
        Ok(Ok(opciones)) => match ejecutar_ingesta(&opciones) {
            Ok(salida) => {
                println!("{}", salida.texto_stdout);
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("Error en lokus_ingestar: {}", err);
                ExitCode::from(1)
            }
        },
        Ok(Err(ayuda)) => {
            println!("{}", ayuda);
            ExitCode::SUCCESS
        }
        Err(err_cli) => {
            eprintln!("Error de argumentos: {}", err_cli);
            ExitCode::from(2)
        }
    }
}
