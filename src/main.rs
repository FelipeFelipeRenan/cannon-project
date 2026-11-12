use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};

/// mimalloc, tuned before its first allocation (see [`tune_once`]).
struct TunedMiMalloc;

static TUNED: AtomicBool = AtomicBool::new(false);

#[inline]
fn tune_once() {
    if !TUNED.load(Ordering::Relaxed) {
        TUNED.store(true, Ordering::Relaxed);
        allocator_tuning();
    }
}

// SAFETY: every method forwards unchanged to `mimalloc::MiMalloc`; the only
// addition is setting allocator options before the first allocation.
unsafe impl GlobalAlloc for TunedMiMalloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        tune_once();
        mimalloc::MiMalloc.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        tune_once();
        mimalloc::MiMalloc.alloc_zeroed(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        mimalloc::MiMalloc.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        tune_once();
        mimalloc::MiMalloc.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: TunedMiMalloc = TunedMiMalloc;

use cannon::app::{self, AppResult};
use cannon::args::parser::Args;
use clap::{CommandFactory, FromArgMatches};
use colored::Colorize;
use std::process::ExitCode;

/// Options of the bundled mimalloc 0.1.49 (see the enum in its `mimalloc.h`).
///
/// They are set by index because the crate does not export names for them;
/// the dependency is pinned and `option_indices_match_the_pinned_mimalloc`
/// checks the layout.
const MI_OPTION_ARENA_EAGER_COMMIT: libmimalloc_sys::mi_option_t = 4;
const MI_OPTION_ALLOW_THP: libmimalloc_sys::mi_option_t = 43;

/// Keeps resident memory close to what is actually used.
///
/// By default mimalloc commits its arenas eagerly and may back them with
/// transparent huge pages, which made 2000 workers use ~45% more memory
/// (175 MB instead of 120 MB) for the same work, with no CPU benefit.
fn allocator_tuning() {
    // SAFETY: plain option setters with valid option numbers; they only
    // change how the allocator requests memory from the OS.
    unsafe {
        libmimalloc_sys::mi_option_set(MI_OPTION_ARENA_EAGER_COMMIT, 0);
        libmimalloc_sys::mi_option_set(MI_OPTION_ALLOW_THP, 0);
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{} {error}", "error:".red().bold());
            ExitCode::FAILURE
        }
    }
}

fn run() -> AppResult<ExitCode> {
    let matches = Args::command().get_matches();
    let mut args = Args::from_arg_matches(&matches)?;

    if args.update {
        #[cfg(feature = "update")]
        {
            app::update::update()?;
            return Ok(ExitCode::SUCCESS);
        }
        #[cfg(not(feature = "update"))]
        return Err("this build of Cannon was compiled without --update support".into());
    }

    if let Some(files) = &args.diff {
        return app::diff::run(files, args.tolerance);
    }

    cannon::args::config::merge_with_yaml(&mut args, &matches)?;
    args.validate()
        .map_err(|error| format!("invalid configuration: {error}"))?;

    app::environment::prepare(&args);

    let runtime = app::environment::build_runtime(&args)?;

    runtime.block_on(app::run(args))
}

#[cfg(test)]
mod tests {
    #[test]
    fn option_indices_match_the_pinned_mimalloc() {
        // Anchors: if the enum in a new mimalloc shifts, these exported
        // values move and this fails instead of silently setting the wrong
        // option. `_mi_option_last` is the number of options (47).
        assert_eq!(libmimalloc_sys::mi_option_large_os_pages, 6);
        assert_eq!(libmimalloc_sys::mi_option_use_numa_nodes, 16);
        assert_eq!(libmimalloc_sys::mi_option_max_warnings, 20);
        assert_eq!(libmimalloc_sys::_mi_option_last, 47);
        assert_eq!(
            super::MI_OPTION_ARENA_EAGER_COMMIT + 2,
            libmimalloc_sys::mi_option_large_os_pages
        );
    }
}
