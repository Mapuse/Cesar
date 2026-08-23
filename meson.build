project('cesar', 'rust',
  version : '0.0.7',
  license : 'MIT',
  default_options : [
    'optimization=3',
    'strip=true',
  ],
)

gen_cross = find_program('scripts/crossgen.sh')
run_command(gen_cross, check : true)

cpu = host_machine.cpu_family()
if cpu == 'x86_64'
  rust_target = 'x86_64-unknown-linux-musl'
elif cpu == 'aarch64'
  rust_target = 'aarch64-unknown-linux-musl'
else
  error('Unsupported architecture: ' + cpu + '. Supported: x86_64, aarch64')
endif

build_dir = meson.project_build_root() / 'cargo'
src_dir = meson.project_source_root()

custom_target('csr',
  build_by_default : true,
  build_always_stale : true,
  install : true,
  install_dir : get_option('prefix') / 'bin',
  command : ['sh', '-c',
    'CARGO_TARGET_DIR={0} cargo build --release --locked --manifest-path {1}/Cargo.toml --target {2} && cp {0}/{2}/release/csr @OUTPUT@'
    .format(build_dir, src_dir, rust_target),
  ],
  output : 'csr',
)

custom_target('csl',
  build_by_default : true,
  build_always_stale : true,
  install : true,
  install_dir : get_option('prefix') / 'bin',
  depends : ['csr'],
  command : ['sh', '-c',
    'cp {0}/{1}/release/csl @OUTPUT@'
    .format(build_dir, rust_target),
  ],
  output : 'csl',
)
