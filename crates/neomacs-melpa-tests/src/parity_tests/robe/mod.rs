use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use neomacs_melpa_test_support::{
    CommandError, elisp_string, output_with_timeout, package_preparation_run_id,
    publish_package_preparation_failure,
};
use sha2::{Digest, Sha256};

use crate::{CachedMelpaOracle, INF_RUBY_MELPA_PIN, ROBE_MELPA_PIN, workspace_root};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ROBE_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const ROBE_RUNTIME_CACHE_VERSION: &str = "ruby-3.3.10-pry-0.14.2-v2";
const ROBE_RUNTIME_COMMAND_TIMEOUT: Duration = Duration::from_secs(240);
const ROBE_RUBY_VERSION: &str = "3.3.10";
const ROBE_NIXPKGS_REVISION: &str = "6368eda62c9775c38ef7f714b2555a741c20c72d";
const WORKSPACE_FLAKE_LOCK: &str =
    include_str!(concat!(env!("CARGO_WORKSPACE_DIR"), "/flake.lock"));

struct PinnedGem {
    name: &'static str,
    version: &'static str,
    sha256: &'static str,
}

const ROBE_GEMS: &[PinnedGem] = &[
    PinnedGem {
        name: "coderay",
        version: "1.1.3",
        sha256: "dc530018a4684512f8f38143cd2a096c9f02a1fc2459edcfe534787a7fc77d4b",
    },
    PinnedGem {
        name: "method_source",
        version: "1.1.0",
        sha256: "181301c9c45b731b4769bc81e8860e72f9161ad7d66dd99103c9ab84f560f5c5",
    },
    PinnedGem {
        name: "pry",
        version: "0.14.2",
        sha256: "c4fe54efedaca1d351280b45b8849af363184696fcac1c72e0415f9bdac4334d",
    },
];

struct RubyRuntimeCommands {
    ruby: Vec<String>,
    gem: Vec<String>,
}

fn command_succeeds(program: &str, arguments: &[&str]) -> bool {
    let mut command = Command::new(program);
    command.args(arguments);
    output_with_timeout(&mut command, Duration::from_secs(15))
        .is_ok_and(|output| output.status.success())
}

fn runtime_command_error(program: &str, error: CommandError) -> String {
    match error {
        CommandError::Launch(error) => {
            format!("failed to launch `{program}` for Robe tests: {error}")
        }
        CommandError::TimedOut(output) => format!(
            "Robe Ruby runtime command `{program}` timed out after {} seconds\nstdout:\n{}\nstderr:\n{}",
            ROBE_RUNTIME_COMMAND_TIMEOUT.as_secs(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        CommandError::Capture(error) => {
            format!("failed to capture `{program}` for Robe tests: {error}")
        }
    }
}

fn run_runtime_command(
    command_prefix: &[String],
    arguments: &[&str],
    gem_home: &Path,
    scratch: &Path,
    working_directory: &Path,
) -> Result<String, String> {
    let (program, prefix) = command_prefix
        .split_first()
        .ok_or_else(|| "empty Robe Ruby runtime command".to_string())?;
    let mut command = Command::new(program);
    command
        .current_dir(working_directory)
        .args(prefix)
        .args(arguments)
        .env("GEM_HOME", gem_home)
        .env("GEM_PATH", gem_home)
        .env("TMPDIR", scratch)
        .env("TMP", scratch)
        .env("TEMP", scratch);
    let output = output_with_timeout(&mut command, ROBE_RUNTIME_COMMAND_TIMEOUT)
        .map_err(|error| runtime_command_error(program, error))?;
    if !output.status.success() {
        return Err(format!(
            "Robe Ruby runtime command failed\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("Robe Ruby runtime emitted non-UTF-8 stdout: {error}"))
}

fn encode_runtime_command(command: &[String]) -> String {
    format!(
        "({})\n",
        command
            .iter()
            .map(|argument| elisp_string(argument))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn select_pinned_ruby_runtime(
    gem_home: &Path,
    scratch: &Path,
    working_directory: &Path,
) -> Result<RubyRuntimeCommands, String> {
    let mut candidates = Vec::new();
    let mut rejected = Vec::new();
    if command_succeeds("nix", &["--version"]) {
        if !WORKSPACE_FLAKE_LOCK.contains(ROBE_NIXPKGS_REVISION) {
            return Err(format!(
                "workspace flake.lock no longer pins the reviewed Robe nixpkgs revision {ROBE_NIXPKGS_REVISION}"
            ));
        }
        let workspace = workspace_root().to_string_lossy().into_owned();
        let realize = vec![
            "nix".to_string(),
            "--no-warn-dirty".to_string(),
            "build".to_string(),
            "--no-link".to_string(),
            "--print-out-paths".to_string(),
            "--inputs-from".to_string(),
            workspace,
            "nixpkgs#ruby_3_3".to_string(),
        ];
        match run_runtime_command(&realize, &[], gem_home, scratch, working_directory) {
            Ok(path) => {
                let path = PathBuf::from(path.trim());
                candidates.push(RubyRuntimeCommands {
                    ruby: vec![path.join("bin/ruby").to_string_lossy().into_owned()],
                    gem: vec![path.join("bin/gem").to_string_lossy().into_owned()],
                });
            }
            Err(error) => rejected.push(error),
        }
    }
    if command_succeeds("ruby", &["--version"]) {
        candidates.push(RubyRuntimeCommands {
            ruby: vec!["ruby".to_string()],
            gem: vec!["ruby".to_string(), "-S".to_string(), "gem".to_string()],
        });
    }
    if candidates.is_empty() {
        return Err(
            "Robe parity needs pinned Ruby 3.3.10, but neither Nix nor Ruby is available"
                .to_string(),
        );
    }

    for commands in candidates {
        match run_runtime_command(
            &commands.ruby,
            &["-e", "print RUBY_VERSION"],
            gem_home,
            scratch,
            working_directory,
        ) {
            Ok(version) if version == ROBE_RUBY_VERSION => return Ok(commands),
            Ok(version) => rejected.push(format!(
                "{} resolved Ruby {version:?}, expected {ROBE_RUBY_VERSION}",
                commands.ruby.join(" ")
            )),
            Err(error) => rejected.push(error),
        }
    }
    Err(format!(
        "no exact Ruby {ROBE_RUBY_VERSION} runtime is available:\n{}",
        rejected.join("\n")
    ))
}

fn verify_artifact_digest(artifact: &Path, expected: &str) -> Result<(), String> {
    let bytes = fs::read(artifact).map_err(|error| {
        format!(
            "failed to read pinned Robe runtime artifact {}: {error}",
            artifact.display()
        )
    })?;
    let actual = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "pinned Robe runtime artifact {} has SHA-256 {actual}, expected {expected}",
            artifact.display()
        ))
    }
}

/// Prepare the supported external Ruby interpreter and exact Pry stack once
/// below `./tmp`.
///
/// Robe's `lib/robe/**` server remains the package payload under test. This
/// cache contains only the external interpreter and its exact Pry dependency.
/// The editor subprocess reads the command vector from `runtime-command.el`,
/// avoiding global environment mutation while GNU Emacs and Neomacs run in
/// parallel.
fn prepare_robe_runtime() -> Result<PathBuf, String> {
    let cache = workspace_root()
        .join("tmp/melpa/tool-cache/robe")
        .join(ROBE_RUNTIME_CACHE_VERSION);
    let artifacts = cache.join("artifacts");
    let gem_home = cache.join("gem-home");
    let scratch = cache.join("tmp");
    let runtime_file = cache.join("runtime-command.el");
    let ready = cache.join("ready");
    let failed = cache.join("failed");
    let runtime_identity = format!(
        "ruby\t{ROBE_RUBY_VERSION}\nnixpkgs\t{ROBE_NIXPKGS_REVISION}\n{}",
        ROBE_GEMS
            .iter()
            .map(|gem| format!("gem\t{}\t{}\t{}\n", gem.name, gem.version, gem.sha256))
            .collect::<String>()
    );
    let failure_prefix = format!(
        "run-id\t{}\nidentity\t{runtime_identity}error\n",
        package_preparation_run_id()
    );

    fs::create_dir_all(&cache)
        .map_err(|error| format!("failed to create Robe runtime cache: {error}"))?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(cache.join("prepare.lock"))
        .map_err(|error| format!("failed to open Robe runtime cache lock: {error}"))?;
    fs4::FileExt::lock(&lock)
        .map_err(|error| format!("failed to lock Robe runtime cache: {error}"))?;

    if let Ok(contents) = fs::read_to_string(&failed)
        && let Some(error) = contents.strip_prefix(&failure_prefix)
    {
        return Err(error.to_string());
    }

    let preparation = (|| -> Result<(), String> {
        for directory in [&artifacts, &gem_home, &scratch] {
            fs::create_dir_all(directory)
                .map_err(|error| format!("failed to create Robe runtime cache: {error}"))?;
        }
        let commands = select_pinned_ruby_runtime(&gem_home, &scratch, &cache)?;
        let encoded_command = encode_runtime_command(&commands.ruby);
        let ready_contract = format!("{runtime_identity}command\t{encoded_command}");
        if fs::read_to_string(&ready).is_ok_and(|contents| contents == ready_contract)
            && fs::read_to_string(&runtime_file).is_ok_and(|contents| contents == encoded_command)
        {
            return Ok(());
        }

        let gem_home_string = gem_home
            .to_str()
            .ok_or_else(|| "Robe gem cache path is not UTF-8".to_string())?;
        for gem in ROBE_GEMS {
            let artifact = artifacts.join(format!("{}-{}.gem", gem.name, gem.version));
            if artifact.exists() && verify_artifact_digest(&artifact, gem.sha256).is_err() {
                fs::remove_file(&artifact).map_err(|error| {
                    format!(
                        "failed to remove invalid Robe runtime artifact {}: {error}",
                        artifact.display()
                    )
                })?;
            }
            if !artifact.is_file() {
                run_runtime_command(
                    &commands.gem,
                    &[
                        "fetch",
                        gem.name,
                        "--version",
                        gem.version,
                        "--clear-sources",
                        "--source",
                        "https://rubygems.org",
                    ],
                    &gem_home,
                    &scratch,
                    &artifacts,
                )?;
            }
            verify_artifact_digest(&artifact, gem.sha256)?;
            let artifact_string = artifact
                .to_str()
                .ok_or_else(|| "Robe gem artifact path is not UTF-8".to_string())?;
            run_runtime_command(
                &commands.gem,
                &[
                    "install",
                    artifact_string,
                    "--local",
                    "--ignore-dependencies",
                    "--no-document",
                    "--install-dir",
                    gem_home_string,
                ],
                &gem_home,
                &scratch,
                &cache,
            )?;
        }

        let versions = run_runtime_command(
            &commands.ruby,
            &[
                "-e",
                "require 'pry'; print [RUBY_VERSION, Pry::VERSION, CodeRay::VERSION, MethodSource::VERSION].join(\"\\t\")",
            ],
            &gem_home,
            &scratch,
            &cache,
        )?;
        if versions.trim().split('\t').collect::<Vec<_>>()
            != [ROBE_RUBY_VERSION, "0.14.2", "1.1.3", "1.1.0"]
        {
            return Err(format!(
                "Robe runtime resolved unexpected exact versions: {versions:?}"
            ));
        }

        let runtime_tmp = cache.join(format!("runtime-command.{}.tmp", std::process::id()));
        let ready_tmp = cache.join(format!("ready.{}.tmp", std::process::id()));
        fs::write(&runtime_tmp, &encoded_command)
            .and_then(|()| fs::rename(&runtime_tmp, &runtime_file))
            .and_then(|()| fs::write(&ready_tmp, ready_contract))
            .and_then(|()| fs::rename(&ready_tmp, &ready))
            .map_err(|error| format!("failed to publish Robe runtime cache: {error}"))?;
        if failed.exists() {
            fs::remove_file(&failed).map_err(|error| {
                format!(
                    "failed to remove stale Robe runtime failure marker {}: {error}",
                    failed.display()
                )
            })?;
        }
        Ok(())
    })();
    if let Err(error) = preparation {
        return Err(publish_package_preparation_failure(
            &failed,
            &failure_prefix,
            error,
        ));
    }
    Ok(runtime_file)
}

const ROBE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'xref)

(defun neomacs-robe-test-yard-eldoc ()
  "Return deterministic documentation from a preexisting provider."
  "YARD: release client")

(defun neomacs-robe-test-write-file (file contents)
  "Write CONTENTS to FILE, creating its parent directory first."
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert contents)))

(defun neomacs-robe-test-root (label)
  "Return a fresh deterministic package sandbox directory for LABEL."
  (let ((root (file-name-as-directory
               (expand-file-name (concat "robe/" label)
                                 (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-robe-test-write-project (root)
  "Write the practical Ruby project loaded by Robe's real Sash server."
  (let ((app (expand-file-name "app.rb" root))
        (definition (expand-file-name "lib/release_plan.rb" root)))
    (neomacs-robe-test-write-file
     app
     (concat
      "plan = Deploy::ReleasePlan.new(owner: \"Ana Ng\", retries: 3)\n"
      "receipt = plan.publish\n"
      "plan.publish!(\"neomacs.tar\", dry_run: false)\n"
      "Deploy::Rel\n"))
    (neomacs-robe-test-write-file
     definition
     (concat
      "module Deploy\n"
      "  class ReleasePlan\n"
      "    attr_reader :owner, :retries\n"
      "\n"
      "    def initialize(owner:, retries: 2)\n"
      "      @owner = owner\n"
      "      @retries = retries\n"
      "    end\n"
      "\n"
      "    # Publish ARTIFACT for the configured owner.\n"
      "    # Returns a stable release receipt.\n"
      "    def publish!(artifact, dry_run: false)\n"
      "      \"#{owner}:#{artifact}:#{dry_run}:#{retries}\"\n"
      "    end\n"
      "\n"
      "    def rollback!(artifact)\n"
      "      \"rolled-back:#{artifact}\"\n"
      "    end\n"
      "  end\n"
      "\n"
      "  class BaseWorkflow\n"
      "    def initialize(name)\n"
      "      @name = name\n"
      "    end\n"
      "\n"
      "    def run(label)\n"
      "      \"#{@name}:#{label}\"\n"
      "    end\n"
      "  end\n"
      "\n"
      "  class WorkflowRelease < BaseWorkflow\n"
      "    def initialize(name)\n"
      "      super\n"
      "    end\n"
      "\n"
      "    def run(label)\n"
      "      super\n"
      "    end\n"
      "  end\n"
      "end\n"))
    (list app definition)))

(defun neomacs-robe-test-runtime-command ()
  "Read the exact external Ruby command prepared below workspace `./tmp`."
  (let ((file (expand-file-name
               "tmp/melpa/tool-cache/robe/ruby-3.3.10-pry-0.14.2-v2/runtime-command.el"
               (getenv "NEOMACS_RUNTIME_ROOT"))))
    (with-temp-buffer
      (insert-file-contents file)
      (goto-char (point-min))
      (let ((read-eval nil)
            (command (read (current-buffer))))
        (skip-chars-forward " \t\r\n")
        (unless (eobp)
          (error "Trailing data in Robe runtime command"))
        (unless (and (consp command) (seq-every-p #'stringp command))
          (error "Malformed Robe runtime command: %S" command))
        command))))

(defun neomacs-robe-test-console-script (root)
  "Write a deterministic line-oriented Ruby evaluator below ROOT."
  (let ((script (expand-file-name "robe-console.rb" root)))
    (neomacs-robe-test-write-file
     script
     (concat
      "$LOAD_PATH.unshift(ARGV.fetch(0))\n"
      "fixture = ARGV.fetch(1)\n"
      "$LOAD_PATH.unshift(File.dirname(fixture))\n"
      "load fixture\n"
      "$stdout.sync = true\n"
      "$stderr.sync = true\n"
      "puts 'NEOMACS-ROBE-CONSOLE:ready'\n"
      "while (line = STDIN.gets)\n"
      "  begin\n"
      "    value = TOPLEVEL_BINDING.eval(line, 'neomacs-robe-console', 1)\n"
      "    puts value.inspect unless value.nil?\n"
      "  rescue Exception => error\n"
      "    warn \"Error: #{error.class}: #{error.message}\"\n"
      "  end\n"
      "end\n"))
    script))

(defun neomacs-robe-test-wait-until (predicate description)
  "Wait until PREDICATE succeeds, with a bounded condition-driven loop."
  (let ((attempts 3000))
    (while (and (> attempts 0) (not (funcall predicate)))
      (setq attempts (1- attempts))
      (accept-process-output nil 0.01))
    (unless (funcall predicate)
      (error "Timed out waiting for %s" description))))

(defun neomacs-robe-test-close-process (process)
  "Gracefully close PROCESS, with bounded forced cleanup as a fallback."
  (when (and (processp process) (process-live-p process))
    (ignore-errors (process-send-eof process))
    (let ((attempts 200))
      (while (and (> attempts 0) (process-live-p process))
        (setq attempts (1- attempts))
        (accept-process-output process 0.01)))
    (when (process-live-p process)
      (delete-process process)))
  (neomacs-robe-test-wait-until
   (lambda () (not (and (processp process) (process-live-p process))))
   "the Robe Ruby console to close")
  (and (processp process) (process-status process)))

(defun neomacs-robe-test-with-console (root definition callback)
  "Call CALLBACK with Robe's real Ruby console buffer and process."
  (let* ((saved-buffers inf-ruby-buffers)
         (saved-buffer inf-ruby-buffer)
         (console (generate-new-buffer " *robe-test-console*"))
         (gem-home
          (expand-file-name
           "tmp/melpa/tool-cache/robe/ruby-3.3.10-pry-0.14.2-v2/gem-home"
           (getenv "NEOMACS_RUNTIME_ROOT")))
         (script (neomacs-robe-test-console-script root))
         process)
    (unwind-protect
        (progn
          (with-current-buffer console
            (setq default-directory (file-name-as-directory root))
            (inf-ruby-mode)
            (let ((process-environment (copy-sequence process-environment)))
              (setenv "GEM_HOME" gem-home)
              (setenv "GEM_PATH" gem-home)
              (setq process
                    (make-process
                     :name "robe-test-console"
                     :buffer console
                     :command
                     (append (neomacs-robe-test-runtime-command)
                             (list script robe-ruby-path definition))
                     :connection-type 'pipe
                     :coding 'utf-8-unix
                     :noquery t))))
          (setq inf-ruby-buffers (list console)
                inf-ruby-buffer console)
          (neomacs-robe-test-wait-until
           (lambda ()
             (or (not (process-live-p process))
                 (with-current-buffer console
                   (save-excursion
                     (goto-char (point-min))
                     (search-forward "NEOMACS-ROBE-CONSOLE:ready" nil t)))))
           "the deterministic Ruby console to become ready")
          (unless (process-live-p process)
            (error "Robe Ruby console exited during startup: %s"
                   (with-current-buffer console (buffer-string))))
          (funcall callback console process))
      (when process
        (neomacs-robe-test-close-process process))
      (setq inf-ruby-buffers saved-buffers
            inf-ruby-buffer saved-buffer)
      (when (buffer-live-p console)
        (with-current-buffer console
          (set-buffer-modified-p nil))
        (kill-buffer console)))))

(defun neomacs-robe-test-access-paths (console)
  "Return the decoded ordered requests from Robe's package-owned server log."
  (let* ((port (buffer-local-value 'robe-port console))
         (file (expand-file-name
                (format "robe-access-%s.log" port)
                temporary-file-directory))
         paths)
    (neomacs-robe-test-wait-until
     (lambda () (file-readable-p file))
     "Robe's access log")
    (with-temp-buffer
      (insert-file-contents file)
      (goto-char (point-min))
      (while (re-search-forward "INFO -- : \\(.*\\)$" nil t)
        (push (match-string-no-properties 1) paths)))
    (nreverse paths)))

(defun neomacs-robe-test-face-runs (text)
  "Return the non-nil face runs in TEXT as exact half-open ranges."
  (let ((position 0)
        runs)
    (while (< position (length text))
      (let* ((face (get-text-property position 'face text))
             (next (next-single-property-change
                    position 'face text (length text))))
        (when face
          (push (list position next face) runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-robe-test-signal (callback)
  "Return CALLBACK's exact nonlocal exit as printable data."
  (condition-case error-data
      (list :value (funcall callback))
    (error (list :signal (car error-data)
                 :data (cdr error-data)
                 :message (error-message-string error-data)))))
"####;

fn robe_oracle() -> CachedMelpaOracle {
    prepare_robe_runtime().expect("prepare exact Ruby/Pry boundary below ./tmp");
    CachedMelpaOracle::new(ROBE_MELPA_PIN, "robe.el")
        .expect("prepare exact shallow Robe source below ./tmp")
        .with_melpa_dependency(INF_RUBY_MELPA_PIN)
        .expect("prepare pinned inf-ruby dependency")
        .with_prelude(ROBE_TEST_PRELUDE)
        .with_timeout(ROBE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Robe parity test")
        .into()
}

fn assert_robe_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(robe_oracle(), &current_test_name(), "robe_parity", cases);
}

#[test]
fn robe_package_batch() {
    assert_robe_batch(&workflows::workflow_batch_cases());
}
