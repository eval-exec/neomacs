use expect_test::expect;

use super::ParityBatchCase;

/// What the package is bound to a volume key for.  `alsamixer-up-volume' reads
/// the current level out of amixer's output, adds the configured step and
/// writes it back, so each press is two commands -- an `sget' then an `sset'
/// -- and the level the second one carries depends on what the first one read.
/// The mixer really moves: 40 up to 45, then back down to 40.  The text the
/// user sees is the command's return value, and it deliberately never reaches
/// *Messages*, because `alsamixer-set-volume' binds `message-log-max' to nil.
fn raises_and_lowers_the_volume_through_amixer() -> ParityBatchCase {
    ParityBatchCase::value(
        "raises_and_lowers_the_volume_through_amixer",
        r##"
        (progn
          (als-test-install-amixer 40 "on")
          (list
           :defaults (list :command alsamixer-amixer-command
                           :control alsamixer-control
                           :step alsamixer-default-volume-increment)
           :starting-volume (alsamixer-get-volume)
           :up (als-test-run (lambda () (alsamixer-up-volume)))
           :after-up (list :reported (alsamixer-get-volume)
                           :mixer (als-test-state))
           :down (als-test-run (lambda () (alsamixer-down-volume)))
           :after-down (list :reported (alsamixer-get-volume)
                             :mixer (als-test-state))
           :commands (als-test-commands)))
    "##,
        expect![[
            r#"OK (:defaults (:command "amixer" :control "Master" :step 5) :starting-volume 40 :up (:shown "Volume set to 45%" :logged-to-messages nil) :after-up (:reported 45 :mixer (45 "on")) :down (:shown "Volume set to 40%" :logged-to-messages nil) :after-down (:reported 40 :mixer (40 "on")) :commands ("amixer sget Master playback" "amixer sget Master playback" "amixer sset Master playback 45%" "amixer sget Master playback" "amixer sget Master playback" "amixer sset Master playback 40%" "amixer sget Master playback"))"#
        ]],
    )
}

fn toggles_mute_but_can_only_ever_report_the_volume() -> ParityBatchCase {
    ParityBatchCase::value(
        "toggles_mute_but_can_only_ever_report_the_volume",
        r##"
        (progn
          (als-test-install-amixer 40 "on")
          (list
           :before (als-test-state)
           :muted (list :returned (alsamixer-toggle-mute)
                        :mixer (als-test-state)
                        :volume-still-reported (alsamixer-get-volume))
           :unmuted (list :returned (alsamixer-toggle-mute)
                          :mixer (als-test-state)
                          :volume-still-reported (alsamixer-get-volume))
           :commands (als-test-commands)))
    "##,
        expect![[
            r#"OK (:before (40 "on") :muted (:returned "Simple mixer control 'Master',0\n  Capabilities: pvolume pswitch pswitch-joined\n  Playback channels: Front Left - Front Right\n  Limits: Playback 0 - 65536\n  Mono:\n  Front Left: Playback 26214 [40%] [-20.00dB] [off]\n  Front Right: Playback 26214 [40%] [-20.00dB] [off]\n" :mixer (40 "off") :volume-still-reported 40) :unmuted (:returned "Simple mixer control 'Master',0\n  Capabilities: pvolume pswitch pswitch-joined\n  Playback channels: Front Left - Front Right\n  Limits: Playback 0 - 65536\n  Mono:\n  Front Left: Playback 26214 [40%] [-20.00dB] [on]\n  Front Right: Playback 26214 [40%] [-20.00dB] [on]\n" :mixer (40 "on") :volume-still-reported 40) :commands ("amixer set Master toggle" "amixer sget Master playback" "amixer set Master toggle" "amixer sget Master playback"))"#
        ]],
    )
}

fn clamps_out_of_range_percentages_and_honours_a_prefix_step() -> ParityBatchCase {
    ParityBatchCase::value(
        "clamps_out_of_range_percentages_and_honours_a_prefix_step",
        r##"
        (progn
          (als-test-install-amixer 50 "on")
          (list
           :below-zero (als-test-run (lambda () (alsamixer-set-volume -10)))
           :after-below (als-test-state)
           :above-hundred (als-test-run (lambda () (alsamixer-set-volume 120)))
           :after-above (als-test-state)
           :prefix-up (progn (alsamixer-set-volume 50)
                             (als-test-run (lambda () (alsamixer-up-volume 20))))
           :prefix-down (als-test-run (lambda () (alsamixer-down-volume 30)))
           :final (list :reported (alsamixer-get-volume)
                        :mixer (als-test-state))
           :commands (als-test-commands)))
    "##,
        expect![[
            r#"OK (:below-zero (:shown "Volume set to 0%" :logged-to-messages nil) :after-below (0 "on") :above-hundred (:shown "Volume set to 100%" :logged-to-messages nil) :after-above (100 "on") :prefix-up (:shown "Volume set to 70%" :logged-to-messages nil) :prefix-down (:shown "Volume set to 40%" :logged-to-messages nil) :final (:reported 40 :mixer (40 "on")) :commands ("amixer sset Master playback 0%" "amixer sset Master playback 100%" "amixer sset Master playback 50%" "amixer sget Master playback" "amixer sset Master playback 70%" "amixer sget Master playback" "amixer sset Master playback 40%" "amixer sget Master playback"))"#
        ]],
    )
}

fn control_card_device_and_step_customizations_change_the_command_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "control_card_device_and_step_customizations_change_the_command_line",
        r##"
        (progn
          (als-test-install-amixer 40 "on")
          (list
           :stock (alsamixer-command "sget %C playback")
           :control-only (let ((alsamixer-control "PCM"))
                           (alsamixer-command "sget %C playback"))
           :card-and-device (let ((alsamixer-card 1)
                                  (alsamixer-device "hw:0"))
                              (alsamixer-command "sget %C playback"))
           :all (let ((alsamixer-control "PCM")
                      (alsamixer-card 1)
                      (alsamixer-device "hw:0")
                      (alsamixer-default-volume-increment 10))
                  (list :built (alsamixer-command "sget %C playback")
                        :up (als-test-run (lambda () (alsamixer-up-volume)))
                        :commands (als-test-commands)))
           :string-card-signals
           (let ((alsamixer-card "1"))
             (als-test-attempt (lambda () (alsamixer-command "sget %C playback"))))
           :declared-card-type (get 'alsamixer-card 'custom-type)))
    "##,
        expect![[
            r#"OK (:stock "amixer sget Master playback" :control-only "amixer sget PCM playback" :card-and-device "amixer -c 1 -D hw:0 sget Master playback" :all (:built "amixer -c 1 -D hw:0 sget PCM playback" :up (:shown "Volume set to 50%" :logged-to-messages nil) :commands ("amixer -c 1 -D hw:0 sget PCM playback" "amixer -c 1 -D hw:0 sset PCM playback 50%")) :string-card-signals (:signal error :message "Format specifier doesn’t match argument type") :declared-card-type string)"#
        ]],
    )
}

fn signals_when_amixer_cannot_be_read_but_never_when_setting() -> ParityBatchCase {
    ParityBatchCase::value(
        "signals_when_amixer_cannot_be_read_but_never_when_setting",
        r##"
        (progn
          (als-test-install-amixer 40 "on")
          (list
           :non-zero-exit
           (progn
             (als-test-force "amixer: Unable to find simple control 'Master',0\n" 1)
             (list :get (als-test-attempt (lambda () (alsamixer-get-volume)))
                   :up (als-test-attempt (lambda () (alsamixer-up-volume)))
                   :set (als-test-attempt (lambda () (alsamixer-set-volume 55)))))
           :no-percentage-in-output
           (progn
             (als-test-force
              "Simple mixer control 'Master',0\n  Capabilities: pvolume\n" 0)
             (als-test-attempt (lambda () (alsamixer-get-volume))))
           :binary-missing
           (progn
             (als-test-force nil nil)
             (als-test-uninstall-amixer)
             (als-test-reset-log)
             (list :found (executable-find "amixer")
                   :get (als-test-attempt (lambda () (alsamixer-get-volume)))
                   :set (als-test-attempt (lambda () (alsamixer-set-volume 55)))
                   :toggle (als-test-attempt (lambda () (alsamixer-toggle-mute)))
                   :commands (als-test-commands)))))
    "##,
        expect![[
            r#"OK (:non-zero-exit (:get (:signal error :message "Unexpected output from amixer: amixer: Unable to find simple control 'Master',0\n") :up (:signal error :message "Unexpected output from amixer: amixer: Unable to find simple control 'Master',0\n") :set (:returned "Volume set to 55%")) :no-percentage-in-output (:signal error :message "Unexpected output from amixer: Simple mixer control 'Master',0\n  Capabilities: pvolume\n") :binary-missing (:found nil :get (:signal error :message "Unexpected output from amixer: [SHELL]: line 1: amixer: command not found\n") :set (:returned "Volume set to 55%") :toggle (:returned "[SHELL]: line 1: amixer: command not found\n") :commands nil))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        raises_and_lowers_the_volume_through_amixer(),
        toggles_mute_but_can_only_ever_report_the_volume(),
        clamps_out_of_range_percentages_and_honours_a_prefix_step(),
        control_card_device_and_step_customizations_change_the_command_line(),
        signals_when_amixer_cannot_be_read_but_never_when_setting(),
    ]
}
