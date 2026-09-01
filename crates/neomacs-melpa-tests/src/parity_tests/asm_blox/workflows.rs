use expect_test::expect;

use super::ParityBatchCase;

/// `M-x asm-blox' -- the only autoloaded entry point the package has, and the
/// screen every session starts on.
///
/// The corpus beside this one covers the engine thoroughly: the parser, the
/// compiler, the runtime, the sources and sinks, the puzzle registry.  What it
/// never does is look at a buffer.  asm-blox is a game played *in* one, so the
/// rendered text is the product, and none of the engine coverage would notice
/// the selection screen drawing nothing, drawing the wrong puzzles, or losing
/// the column alignment that makes it readable.
///
/// Asserted whole rather than sampled: every puzzle row, its difficulty, and
/// its description truncated to the column width.
fn the_puzzle_selection_screen_renders_every_puzzle_with_its_difficulty() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_puzzle_selection_screen_renders_every_puzzle_with_its_difficulty",
        r##"(progn
  (asm-blox)
  (list :buffer (buffer-name)
        :mode major-mode
        :point (point)
        :text (buffer-substring-no-properties (point-min) (point-max))))"##,
        expect![[
            r#"OK (:buffer "*asm-blox-puzzle-selection*" :mode fundamental-mode :point 1 :text "[ ] tutorial Constant Generator        Repeatedly send the number 1 to N. There are no inputs.        \n[ ] tutorial Identity                  Take an input from the input X and send it to the output X.    \n[ ] tutorial Diagnostic Test           Send data from A to X. Send data from B to Y.                  \n[ ] tutorial Signal Amplifier          Read a value from I. Double it. Send that to O.                \n[ ] tutorial Differential Converter    Read a value from A and B. Send A - B to P. Send B - A to N.   \n[ ] tutorial Signal Comparator         Read a value from I. If I > 0 send 1 to G. If I < 0 send 1 …   \n[ ] easy     Number Addition           Take input from A, B, and C, add the three together, and se…   \n[ ] easy     Number Filter             Read a value from I. If it is even send 0 to O, else send t…   \n[ ] easy     Number Sum                Read a number from I, send to O the sum of numbers from 0 t…   \n[ ] easy     Number Chooser            Take an input from A and B. If A>B then send A to L, 0 to R…   \n[ ] easy     Clock Hours               On a clock with hours 0 to 23, read a value from H and add …   \n[ ] easy     Upcase                    Read a character from C and send it to O, upcasing it if it…   \n[ ] easy     Editor Basics             <editor> Write the string \"Hello World\" to the editor.         \n[ ] easy     Triangle Area             Read base and height of a right-triangle from B and H respe…   \n[ ] easy     Sequence Generator        Read a value from A and B. Send the lesser of the two to O.…   \n[ ] easy     Sequence Counter          Read a 0-terminated sequence from I. Write the length of th…   \n[ ] easy     Signal Edge Detector      Read a value from I, comparing it with the previous value. …   \n[ ] easy     Interrupt Handler         Read inputs 1, 2, 3, and 4. Whenever an input sequence chan…   \n[ ] medium   Indentation I             <editor> Edit text to match the target.                        \n[ ] medium   List Length               Lists are 0 terminated. Read a list from I, calculate its l…   \n[ ] medium   List Reverse              Lists are 0 terminated. Read a list from L, reverse it, and…   \n[ ] medium   Increment Cout            Return the number of times subsequent values of I increase.…   \n[ ] medium   Simple Graph              <editor> Read a number from A,  draw a line with that many …   \n[ ] medium   Signal Pattern Detector   Read a value from I. Find the pattern 0, 0, 0: - If the cur…   \n[ ] medium   Sequence Peak Detector    Read a 0-terminated sequence from I. Write the minimum valu…   \n[ ] medium   Sequence Reverser         Read a sequence from I. Reverse the sequence and write it t…   \n[ ] medium   Signal Multiplier         Read a value from A and B. Multiply the numbers. Send resul…   \n[ ] hard     Merge Step                Numbers in A and B are sorted. Read numbers from A and B, c…   \n[ ] hard     Meeting point             Read the 10 numbers from N (n1, n2, ..., n10). Send a numbe…   \n[ ] hard     Turing                    Read a number from X. Implement a machine that moves a head…   \n[ ] hard     Stack Machine             Read all 40 values from A, pushing them on a stack so that …   \n[ ] hard     Delete Word               <editor> Read 0-based index from I. Delete that number word…   \n[ ] hard     Signal Window Filter      Read a value from I. Write the sum of the last 3 values to …   \n[ ] hard     Signal Divider            Read a value from A and B. Send the quotient of A / B to Q.…   \n[ ] hard     Sequence Indexer          Read a 0-terminated sequence from D,   storing it to be acc…   \n[ ] hard     Sequence Sorter           Read a 0-terminated sequence from I. Sort the sequence and …   \n")"#
        ]],
    )
}

fn choosing_a_puzzle_draws_its_board_and_typing_stays_inside_one_code_box() -> ParityBatchCase {
    ParityBatchCase::value(
        "choosing_a_puzzle_draws_its_board_and_typing_stays_inside_one_code_box",
        r##"(progn
  (asm-blox)
  (goto-char (point-min))
  (asm-blox-select-puzzle)
  (set-window-buffer (selected-window) (current-buffer))
  (let* ((board (buffer-name))
         (mode major-mode)
         (drawn (buffer-substring-no-properties (point-min) (point-max)))
         (first-box (next-single-property-change (point-min) 'asm-blox-box-id)))
    (goto-char first-box)
    (execute-kbd-macro (string-to-vector "UP 1"))
    (let ((typed (buffer-substring-no-properties (point-min) (point-max))))
      (list :board board
            :mode mode
            :first-box-position first-box
            :typing-changed-the-board (not (equal drawn typed))
            :typed-text-visible (and (string-match-p "UP 1" typed) t)
            :line-count-unchanged
            (= (length (split-string drawn "\n"))
               (length (split-string typed "\n")))
            :board-after-typing typed))))"##,
        expect![[
            r#"OK (:board "Constant Generator-1.asbx" :mode asm-blox-mode :first-box-position 373 :typing-changed-the-board t :typed-text-visible t :line-count-unchanged t :board-after-typing "                                                                                                                       \n                                                                                                                       \n           ┌────────────────────┐     ┌────────────────────┐     ┌────────────────────┐     ┌────────────────────┐     \n           │UP 1                │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │  →  │                    │  →  │                    │  →  │                    │ →N  \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │  ←  │                    │  ←  │                    │  ←  │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           └────────────────────┘     └────────────────────┘     └────────────────────┘     └────────────────────┘     \n                     ↑ ↓                        ↑ ↓                        ↑ ↓                        ↑ ↓              \n           ┌────────────────────┐     ┌────────────────────┐     ┌────────────────────┐     ┌────────────────────┐     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │  →  │                    │  →  │                    │  →  │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │  ←  │                    │  ←  │                    │  ←  │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           └────────────────────┘     └────────────────────┘     └────────────────────┘     └────────────────────┘     \n                     ↑ ↓                        ↑ ↓                        ↑ ↓                        ↑ ↓              \n           ┌────────────────────┐     ┌────────────────────┐     ┌────────────────────┐     ┌────────────────────┐     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │  →  │                    │  →  │                    │  →  │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │  ←  │                    │  ←  │                    │  ←  │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           │                    │     │                    │     │                    │     │                    │     \n           └────────────────────┘     └────────────────────┘     └────────────────────┘     └────────────────────┘     \n                                                                                                                       \n                                                                                                                       \n\n\nConstant Generator:\nRepeatedly send the number 1 to N. There are no inputs.\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_puzzle_selection_screen_renders_every_puzzle_with_its_difficulty(),
        choosing_a_puzzle_draws_its_board_and_typing_stays_inside_one_code_box(),
    ]
}
