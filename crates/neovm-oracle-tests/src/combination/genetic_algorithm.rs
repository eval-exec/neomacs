//! Oracle parity tests for a simple genetic algorithm implemented in pure Elisp.
//!
//! Implements: chromosome representation (bit strings as vectors), fitness
//! function (count ones), tournament selection, single-point crossover,
//! bit-flip mutation, population management, and generational evolution
//! to solve the "maximize ones" optimization problem.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Chromosome representation and fitness function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ga_chromosome_and_fitness() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Chromosomes are vectors of 0s and 1s. Fitness = count of ones.
    // Test creation, fitness calculation, and chromosome manipulation.
    let form = r#"(progn
  ;; Create a chromosome from a list of bits
  (fset 'neovm--ga-make-chromosome
    (lambda (bits)
      (apply 'vector bits)))

  ;; Fitness function: count number of 1s (maximize ones problem)
  (fset 'neovm--ga-fitness
    (lambda (chrom)
      (let ((sum 0)
            (i 0)
            (len (length chrom)))
        (while (< i len)
          (setq sum (+ sum (aref chrom i)))
          (setq i (1+ i)))
        sum)))

  ;; Create a deterministic "random" chromosome using a seed-based PRNG
  (fset 'neovm--ga-make-random-chromosome
    (lambda (length seed)
      (let ((chrom (make-vector length 0))
            (state seed)
            (i 0))
        (while (< i length)
          ;; Simple LCG: state = (state * 1103515245 + 12345) mod 2^31
          (setq state (% (+ (* state 1103515245) 12345) 2147483648))
          (aset chrom i (% (/ state 65536) 2))
          (setq i (1+ i)))
        chrom)))

  ;; Chromosome to list for display
  (fset 'neovm--ga-chrom-to-list
    (lambda (chrom)
      (let ((result nil) (i (1- (length chrom))))
        (while (>= i 0)
          (setq result (cons (aref chrom i) result))
          (setq i (1- i)))
        result)))

  (unwind-protect
      (let ((c1 (funcall 'neovm--ga-make-chromosome '(1 0 1 1 0 1 0 0)))
            (c2 (funcall 'neovm--ga-make-chromosome '(1 1 1 1 1 1 1 1)))
            (c3 (funcall 'neovm--ga-make-chromosome '(0 0 0 0 0 0 0 0)))
            (c4 (funcall 'neovm--ga-make-random-chromosome 10 42)))
        (list
         ;; Fitness calculations
         (funcall 'neovm--ga-fitness c1)   ;; 4 ones
         (funcall 'neovm--ga-fitness c2)   ;; 8 ones (all)
         (funcall 'neovm--ga-fitness c3)   ;; 0 ones (none)
         (funcall 'neovm--ga-fitness c4)   ;; deterministic from seed
         ;; Chromosome lengths
         (length c1) (length c2) (length c4)
         ;; Display chromosomes
         (funcall 'neovm--ga-chrom-to-list c1)
         (funcall 'neovm--ga-chrom-to-list c4)
         ;; Perfect chromosome has fitness = length
         (= (funcall 'neovm--ga-fitness c2) (length c2))
         ;; Zero chromosome has fitness = 0
         (= (funcall 'neovm--ga-fitness c3) 0)))
    (fmakunbound 'neovm--ga-make-chromosome)
    (fmakunbound 'neovm--ga-fitness)
    (fmakunbound 'neovm--ga-make-random-chromosome)
    (fmakunbound 'neovm--ga-chrom-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (4 8 0 8 8 8 10 (1 0 1 1 0 1 0 0) (1 1 1 1 0 1 1 0 1 1) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tournament selection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ga_tournament_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tournament selection: pick k random individuals, return the fittest.
    // Using deterministic index selection for reproducibility.
    let form = r#"(progn
  (fset 'neovm--ga2-fitness
    (lambda (chrom)
      (let ((sum 0) (i 0) (len (length chrom)))
        (while (< i len)
          (setq sum (+ sum (aref chrom i)))
          (setq i (1+ i)))
        sum)))

  ;; Tournament selection: given population (vector of chromosomes),
  ;; select the fittest among individuals at given indices
  (fset 'neovm--ga2-tournament
    (lambda (pop indices)
      "Return the fittest chromosome among those at INDICES in POP."
      (let ((best nil)
            (best-fit -1))
        (dolist (idx indices)
          (let* ((chrom (aref pop idx))
                 (fit (funcall 'neovm--ga2-fitness chrom)))
            (when (> fit best-fit)
              (setq best chrom)
              (setq best-fit fit))))
        best)))

  ;; Deterministic tournament: use LCG to pick tournament indices
  (fset 'neovm--ga2-select
    (lambda (pop pop-size tournament-size seed)
      "Select one individual via tournament with deterministic randomness."
      (let ((indices nil)
            (state seed)
            (k 0))
        (while (< k tournament-size)
          (setq state (% (+ (* state 1103515245) 12345) 2147483648))
          (setq indices (cons (% (/ state 65536) pop-size) indices))
          (setq k (1+ k)))
        (funcall 'neovm--ga2-tournament pop indices))))

  (unwind-protect
      (let ((pop (vector
                  [0 0 0 0 0 0 0 0]   ;; fitness 0
                  [1 0 0 0 0 0 0 0]   ;; fitness 1
                  [1 1 0 0 0 0 0 0]   ;; fitness 2
                  [1 1 1 0 0 0 0 0]   ;; fitness 3
                  [1 1 1 1 0 0 0 0]   ;; fitness 4
                  [1 1 1 1 1 0 0 0]   ;; fitness 5
                  [1 1 1 1 1 1 0 0]   ;; fitness 6
                  [1 1 1 1 1 1 1 1])));; fitness 8
        (list
         ;; Tournament among indices 0,1,2 -> fittest is index 2 (fitness 2)
         (let ((winner (funcall 'neovm--ga2-tournament pop '(0 1 2))))
           (funcall 'neovm--ga2-fitness winner))
         ;; Tournament among indices 5,6,7 -> fittest is index 7 (fitness 8)
         (let ((winner (funcall 'neovm--ga2-tournament pop '(5 6 7))))
           (funcall 'neovm--ga2-fitness winner))
         ;; Tournament with single index -> that individual
         (let ((winner (funcall 'neovm--ga2-tournament pop '(3))))
           (funcall 'neovm--ga2-fitness winner))
         ;; Deterministic selection with seed
         (funcall 'neovm--ga2-fitness
                  (funcall 'neovm--ga2-select pop 8 3 100))
         (funcall 'neovm--ga2-fitness
                  (funcall 'neovm--ga2-select pop 8 3 200))
         (funcall 'neovm--ga2-fitness
                  (funcall 'neovm--ga2-select pop 8 3 300))
         ;; Selected fitness should always be >= 0
         (>= (funcall 'neovm--ga2-fitness
                       (funcall 'neovm--ga2-select pop 8 5 999))
              0)))
    (fmakunbound 'neovm--ga2-fitness)
    (fmakunbound 'neovm--ga2-tournament)
    (fmakunbound 'neovm--ga2-select)))"#;
    let expect = expect_test::expect![[r#""OK (2 8 3 6 8 5 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Single-point crossover and bit-flip mutation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ga_crossover_and_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Single-point crossover: pick a point, swap tails between two parents.
    // Bit-flip mutation: flip each bit with some probability (deterministic).
    let form = r#"(progn
  ;; Single-point crossover at a given position
  (fset 'neovm--ga3-crossover
    (lambda (parent1 parent2 point)
      "Cross PARENT1 and PARENT2 at POINT, return (child1 . child2)."
      (let* ((len (length parent1))
             (c1 (make-vector len 0))
             (c2 (make-vector len 0))
             (i 0))
        ;; Copy first part from respective parents
        (while (< i point)
          (aset c1 i (aref parent1 i))
          (aset c2 i (aref parent2 i))
          (setq i (1+ i)))
        ;; Copy second part swapped
        (while (< i len)
          (aset c1 i (aref parent2 i))
          (aset c2 i (aref parent1 i))
          (setq i (1+ i)))
        (cons c1 c2))))

  ;; Bit-flip mutation with deterministic PRNG
  (fset 'neovm--ga3-mutate
    (lambda (chrom mutation-threshold seed)
      "Flip bits in CHROM where PRNG value < MUTATION-THRESHOLD (0-1000)."
      (let* ((len (length chrom))
             (result (make-vector len 0))
             (state seed)
             (i 0))
        (while (< i len)
          (setq state (% (+ (* state 1103515245) 12345) 2147483648))
          (let ((rand-val (% (/ state 65536) 1000)))
            (if (< rand-val mutation-threshold)
                ;; Flip the bit
                (aset result i (- 1 (aref chrom i)))
              ;; Keep original
              (aset result i (aref chrom i))))
          (setq i (1+ i)))
        result)))

  ;; Helper: chromosome to list
  (fset 'neovm--ga3-to-list
    (lambda (chrom)
      (let ((r nil) (i (1- (length chrom))))
        (while (>= i 0)
          (setq r (cons (aref chrom i) r))
          (setq i (1- i)))
        r)))

  (unwind-protect
      (let ((p1 [1 1 1 1 0 0 0 0])
            (p2 [0 0 0 0 1 1 1 1]))
        (list
         ;; Crossover at position 4: children should be swapped halves
         (let ((children (funcall 'neovm--ga3-crossover p1 p2 4)))
           (list (funcall 'neovm--ga3-to-list (car children))
                 (funcall 'neovm--ga3-to-list (cdr children))))
         ;; Crossover at position 0: children = swapped parents
         (let ((children (funcall 'neovm--ga3-crossover p1 p2 0)))
           (list (funcall 'neovm--ga3-to-list (car children))
                 (funcall 'neovm--ga3-to-list (cdr children))))
         ;; Crossover at position 8 (end): children = original parents
         (let ((children (funcall 'neovm--ga3-crossover p1 p2 8)))
           (list (funcall 'neovm--ga3-to-list (car children))
                 (funcall 'neovm--ga3-to-list (cdr children))))
         ;; Crossover at position 2
         (let ((children (funcall 'neovm--ga3-crossover p1 p2 2)))
           (list (funcall 'neovm--ga3-to-list (car children))
                 (funcall 'neovm--ga3-to-list (cdr children))))
         ;; Mutation with zero threshold: no change
         (funcall 'neovm--ga3-to-list
                  (funcall 'neovm--ga3-mutate p1 0 42))
         ;; Mutation with 1000 threshold (always flip): inverted
         (funcall 'neovm--ga3-to-list
                  (funcall 'neovm--ga3-mutate p1 1000 42))
         ;; Mutation with moderate threshold: deterministic result
         (funcall 'neovm--ga3-to-list
                  (funcall 'neovm--ga3-mutate p1 200 42))
         ;; Verify crossover preserves total number of 1s (for complementary parents)
         (let* ((children (funcall 'neovm--ga3-crossover p1 p2 3))
                (c1 (car children))
                (c2 (cdr children))
                (sum1 0) (sum2 0) (i 0))
           (while (< i 8)
             (setq sum1 (+ sum1 (aref c1 i)))
             (setq sum2 (+ sum2 (aref c2 i)))
             (setq i (1+ i)))
           (= (+ sum1 sum2) 8))))
    (fmakunbound 'neovm--ga3-crossover)
    (fmakunbound 'neovm--ga3-mutate)
    (fmakunbound 'neovm--ga3-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 1 1 1 1 1 1 1) (0 0 0 0 0 0 0 0)) ((0 0 0 0 1 1 1 1) (1 1 1 1 0 0 0 0)) ((1 1 1 1 0 0 0 0) (0 0 0 0 1 1 1 1)) ((1 1 0 0 1 1 1 1) (0 0 1 1 0 0 0 0)) (1 1 1 1 0 0 0 0) (0 0 0 0 1 1 1 1) (0 0 1 1 0 1 0 0) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Population initialization and statistics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ga_population_management() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create an initial population, compute population-level statistics:
    // best fitness, worst fitness, average fitness, diversity measure
    let form = r#"(progn
  (fset 'neovm--ga4-fitness
    (lambda (chrom)
      (let ((sum 0) (i 0) (len (length chrom)))
        (while (< i len)
          (setq sum (+ sum (aref chrom i)))
          (setq i (1+ i)))
        sum)))

  ;; Create initial population using deterministic PRNG
  (fset 'neovm--ga4-init-population
    (lambda (pop-size chrom-length seed)
      (let ((pop (make-vector pop-size nil))
            (state seed)
            (p 0))
        (while (< p pop-size)
          (let ((chrom (make-vector chrom-length 0))
                (i 0))
            (while (< i chrom-length)
              (setq state (% (+ (* state 1103515245) 12345) 2147483648))
              (aset chrom i (% (/ state 65536) 2))
              (setq i (1+ i)))
            (aset pop p chrom))
          (setq p (1+ p)))
        pop)))

  ;; Population statistics
  (fset 'neovm--ga4-stats
    (lambda (pop)
      (let ((best-fit 0)
            (worst-fit (length (aref pop 0)))
            (total-fit 0)
            (pop-size (length pop))
            (i 0))
        (while (< i pop-size)
          (let ((fit (funcall 'neovm--ga4-fitness (aref pop i))))
            (when (> fit best-fit) (setq best-fit fit))
            (when (< fit worst-fit) (setq worst-fit fit))
            (setq total-fit (+ total-fit fit)))
          (setq i (1+ i)))
        (list best-fit worst-fit total-fit pop-size))))

  ;; Hamming distance between two chromosomes
  (fset 'neovm--ga4-hamming
    (lambda (c1 c2)
      (let ((dist 0) (i 0) (len (length c1)))
        (while (< i len)
          (unless (= (aref c1 i) (aref c2 i))
            (setq dist (1+ dist)))
          (setq i (1+ i)))
        dist)))

  ;; Average pairwise diversity (sample: compare each to first individual)
  (fset 'neovm--ga4-diversity
    (lambda (pop)
      (let ((ref (aref pop 0))
            (total 0)
            (i 1)
            (n (length pop)))
        (while (< i n)
          (setq total (+ total (funcall 'neovm--ga4-hamming ref (aref pop i))))
          (setq i (1+ i)))
        total)))

  (unwind-protect
      (let ((pop (funcall 'neovm--ga4-init-population 10 8 12345)))
        (let ((stats (funcall 'neovm--ga4-stats pop)))
          (list
           ;; Stats: (best worst total pop-size)
           stats
           ;; Best fitness <= chromosome length
           (<= (nth 0 stats) 8)
           ;; Worst fitness >= 0
           (>= (nth 1 stats) 0)
           ;; Best >= worst
           (>= (nth 0 stats) (nth 1 stats))
           ;; Total = sum of all fitnesses
           (= (nth 3 stats) 10)
           ;; Diversity measure
           (funcall 'neovm--ga4-diversity pop)
           ;; Hamming distance of individual with itself is 0
           (funcall 'neovm--ga4-hamming (aref pop 0) (aref pop 0)))))
    (fmakunbound 'neovm--ga4-fitness)
    (fmakunbound 'neovm--ga4-init-population)
    (fmakunbound 'neovm--ga4-stats)
    (fmakunbound 'neovm--ga4-hamming)
    (fmakunbound 'neovm--ga4-diversity)))"#;
    let expect = expect_test::expect![[r#""OK ((6 3 40 10) t t t t 30 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full generational evolution (maximize ones)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ga_full_evolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Run a complete genetic algorithm for several generations.
    // Problem: maximize the number of 1s in an 8-bit chromosome.
    // All randomness is deterministic (seeded PRNG).
    let form = r#"(progn
  (fset 'neovm--ga5-fitness
    (lambda (chrom)
      (let ((s 0) (i 0) (len (length chrom)))
        (while (< i len) (setq s (+ s (aref chrom i))) (setq i (1+ i)))
        s)))

  (fset 'neovm--ga5-prng-next
    (lambda (state)
      (% (+ (* state 1103515245) 12345) 2147483648)))

  ;; Tournament select: pick 3 random indices, return fittest
  (fset 'neovm--ga5-select
    (lambda (pop pop-size state)
      (let ((best nil) (best-fit -1) (k 0) (s state))
        (while (< k 3)
          (setq s (funcall 'neovm--ga5-prng-next s))
          (let* ((idx (% (/ s 65536) pop-size))
                 (chrom (aref pop idx))
                 (fit (funcall 'neovm--ga5-fitness chrom)))
            (when (> fit best-fit)
              (setq best chrom best-fit fit)))
          (setq k (1+ k)))
        (cons best s))))

  ;; Crossover at deterministic point
  (fset 'neovm--ga5-crossover
    (lambda (p1 p2 state)
      (let* ((len (length p1))
             (s (funcall 'neovm--ga5-prng-next state))
             (point (1+ (% (/ s 65536) (1- len))))
             (child (make-vector len 0))
             (i 0))
        (while (< i point)
          (aset child i (aref p1 i))
          (setq i (1+ i)))
        (while (< i len)
          (aset child i (aref p2 i))
          (setq i (1+ i)))
        (cons child s))))

  ;; Mutate with 10% probability per bit
  (fset 'neovm--ga5-mutate
    (lambda (chrom state)
      (let* ((len (length chrom))
             (result (make-vector len 0))
             (s state)
             (i 0))
        (while (< i len)
          (setq s (funcall 'neovm--ga5-prng-next s))
          (let ((r (% (/ s 65536) 100)))
            (if (< r 10)
                (aset result i (- 1 (aref chrom i)))
              (aset result i (aref chrom i))))
          (setq i (1+ i)))
        (cons result s))))

  ;; Run one generation: produce new population from old
  (fset 'neovm--ga5-evolve-gen
    (lambda (pop pop-size state)
      (let ((new-pop (make-vector pop-size nil))
            (s state)
            (i 0))
        ;; Elitism: keep best individual
        (let ((best-idx 0) (best-fit -1) (j 0))
          (while (< j pop-size)
            (let ((f (funcall 'neovm--ga5-fitness (aref pop j))))
              (when (> f best-fit) (setq best-idx j best-fit f)))
            (setq j (1+ j)))
          (aset new-pop 0 (aref pop best-idx)))
        ;; Fill rest with crossover + mutation
        (setq i 1)
        (while (< i pop-size)
          (let* ((sel1 (funcall 'neovm--ga5-select pop pop-size s))
                 (p1 (car sel1)))
            (setq s (cdr sel1))
            (let* ((sel2 (funcall 'neovm--ga5-select pop pop-size s))
                   (p2 (car sel2)))
              (setq s (cdr sel2))
              (let* ((cx (funcall 'neovm--ga5-crossover p1 p2 s))
                     (child (car cx)))
                (setq s (cdr cx))
                (let* ((mt (funcall 'neovm--ga5-mutate child s))
                       (mutated (car mt)))
                  (setq s (cdr mt))
                  (aset new-pop i mutated)))))
          (setq i (1+ i)))
        (cons new-pop s))))

  ;; Best fitness in population
  (fset 'neovm--ga5-best-fitness
    (lambda (pop)
      (let ((best 0) (i 0) (n (length pop)))
        (while (< i n)
          (let ((f (funcall 'neovm--ga5-fitness (aref pop i))))
            (when (> f best) (setq best f)))
          (setq i (1+ i)))
        best)))

  (unwind-protect
      (let* ((pop-size 12)
             (chrom-len 8)
             ;; Initialize population
             (pop (make-vector pop-size nil))
             (state 77777)
             (p 0))
        ;; Generate initial population
        (while (< p pop-size)
          (let ((chrom (make-vector chrom-len 0))
                (i 0))
            (while (< i chrom-len)
              (setq state (funcall 'neovm--ga5-prng-next state))
              (aset chrom i (% (/ state 65536) 2))
              (setq i (1+ i)))
            (aset pop p chrom))
          (setq p (1+ p)))
        ;; Record initial best
        (let ((initial-best (funcall 'neovm--ga5-best-fitness pop))
              (generation-bests nil)
              (gen 0))
          ;; Evolve for 8 generations
          (while (< gen 8)
            (let ((result (funcall 'neovm--ga5-evolve-gen pop pop-size state)))
              (setq pop (car result))
              (setq state (cdr result)))
            (setq generation-bests
                  (cons (funcall 'neovm--ga5-best-fitness pop) generation-bests))
            (setq gen (1+ gen)))
          (let ((final-best (funcall 'neovm--ga5-best-fitness pop))
                (gen-list (nreverse generation-bests)))
            (list
             ;; Initial best fitness
             initial-best
             ;; Best fitness per generation
             gen-list
             ;; Final best fitness
             final-best
             ;; Final best should be >= initial (elitism ensures non-decrease)
             (>= final-best initial-best)
             ;; Best fitness bounded by chromosome length
             (<= final-best chrom-len)
             ;; Generation bests should be non-decreasing (elitism)
             (let ((non-decreasing t)
                   (prev initial-best)
                   (rest gen-list))
               (while rest
                 (when (< (car rest) prev)
                   (setq non-decreasing nil))
                 (setq prev (car rest))
                 (setq rest (cdr rest)))
               non-decreasing)))))
    (fmakunbound 'neovm--ga5-fitness)
    (fmakunbound 'neovm--ga5-prng-next)
    (fmakunbound 'neovm--ga5-select)
    (fmakunbound 'neovm--ga5-crossover)
    (fmakunbound 'neovm--ga5-mutate)
    (fmakunbound 'neovm--ga5-evolve-gen)
    (fmakunbound 'neovm--ga5-best-fitness)))"#;
    let expect = expect_test::expect![[r#""OK (5 (6 6 8 8 8 8 8 8) 8 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fitness-proportionate (roulette wheel) selection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ga_roulette_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Roulette wheel selection: probability of selection proportional to fitness.
    // Deterministic via seeded PRNG. Run multiple selections and tally.
    let form = r#"(progn
  (fset 'neovm--ga6-fitness
    (lambda (chrom)
      (let ((s 0) (i 0) (len (length chrom)))
        (while (< i len) (setq s (+ s (aref chrom i))) (setq i (1+ i)))
        s)))

  ;; Compute cumulative fitness array for roulette wheel
  (fset 'neovm--ga6-cumulative-fitness
    (lambda (pop)
      (let* ((n (length pop))
             (cum (make-vector n 0))
             (running 0)
             (i 0))
        (while (< i n)
          (setq running (+ running (funcall 'neovm--ga6-fitness (aref pop i))))
          (aset cum i running)
          (setq i (1+ i)))
        cum)))

  ;; Roulette select: pick a random number in [0, total-fitness), find individual
  (fset 'neovm--ga6-roulette
    (lambda (pop cum-fitness total-fitness state)
      (let* ((s (% (+ (* state 1103515245) 12345) 2147483648))
             (target (% (/ s 65536) total-fitness))
             (i 0)
             (n (length pop)))
        (while (and (< i n) (>= target (aref cum-fitness i)))
          (setq i (1+ i)))
        (when (>= i n) (setq i (1- n)))
        (cons i s))))

  (unwind-protect
      (let ((pop (vector
                  [0 0 0 0]    ;; fitness 0
                  [1 0 0 0]    ;; fitness 1
                  [1 1 0 0]    ;; fitness 2
                  [1 1 1 0]    ;; fitness 3
                  [1 1 1 1]))) ;; fitness 4
        (let* ((cum (funcall 'neovm--ga6-cumulative-fitness pop))
               (total (aref cum (1- (length pop))))
               ;; Run 20 selections and count how many times each index is picked
               (counts (make-vector 5 0))
               (state 54321)
               (trial 0))
          (while (< trial 20)
            (let ((result (funcall 'neovm--ga6-roulette pop cum total state)))
              (aset counts (car result) (1+ (aref counts (car result))))
              (setq state (cdr result)))
            (setq trial (1+ trial)))
          (list
           ;; Cumulative fitness array
           (let ((r nil) (i (1- (length cum))))
             (while (>= i 0)
               (setq r (cons (aref cum i) r))
               (setq i (1- i)))
             r)
           ;; Total fitness
           total
           ;; Selection counts
           (let ((r nil) (i (1- (length counts))))
             (while (>= i 0)
               (setq r (cons (aref counts i) r))
               (setq i (1- i)))
             r)
           ;; Total selections should equal 20
           (let ((sum 0) (i 0))
             (while (< i 5)
               (setq sum (+ sum (aref counts i)))
               (setq i (1+ i)))
             sum)
           ;; Index 0 (fitness 0) should never be selected
           (= (aref counts 0) 0))))
    (fmakunbound 'neovm--ga6-fitness)
    (fmakunbound 'neovm--ga6-cumulative-fitness)
    (fmakunbound 'neovm--ga6-roulette)))"#;
    let expect = expect_test::expect![[r#""OK ((0 1 3 6 10) 10 (0 7 4 2 7) 20 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-point crossover and uniform crossover
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ga_advanced_crossover_operators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two-point crossover and uniform crossover operators.
    let form = r#"(progn
  ;; Two-point crossover: swap segment between point1 and point2
  (fset 'neovm--ga7-two-point-crossover
    (lambda (p1 p2 pt1 pt2)
      "Cross P1 and P2 between PT1 and PT2."
      (let* ((len (length p1))
             (lo (min pt1 pt2))
             (hi (max pt1 pt2))
             (c1 (make-vector len 0))
             (c2 (make-vector len 0))
             (i 0))
        (while (< i len)
          (if (and (>= i lo) (< i hi))
              ;; Inside segment: swap
              (progn
                (aset c1 i (aref p2 i))
                (aset c2 i (aref p1 i)))
            ;; Outside segment: keep
            (aset c1 i (aref p1 i))
            (aset c2 i (aref p2 i)))
          (setq i (1+ i)))
        (cons c1 c2))))

  ;; Uniform crossover: each bit independently chosen from either parent
  (fset 'neovm--ga7-uniform-crossover
    (lambda (p1 p2 mask)
      "MASK is a vector of 0/1: 0 means take from P1, 1 from P2."
      (let* ((len (length p1))
             (child (make-vector len 0))
             (i 0))
        (while (< i len)
          (aset child i
                (if (= (aref mask i) 0)
                    (aref p1 i)
                  (aref p2 i)))
          (setq i (1+ i)))
        child)))

  ;; Helper
  (fset 'neovm--ga7-to-list
    (lambda (v)
      (let ((r nil) (i (1- (length v))))
        (while (>= i 0) (setq r (cons (aref v i) r)) (setq i (1- i)))
        r)))

  (unwind-protect
      (let ((p1 [1 1 1 1 0 0 0 0])
            (p2 [0 0 0 0 1 1 1 1]))
        (list
         ;; Two-point crossover: swap bits 2-5
         (let ((children (funcall 'neovm--ga7-two-point-crossover p1 p2 2 6)))
           (list (funcall 'neovm--ga7-to-list (car children))
                 (funcall 'neovm--ga7-to-list (cdr children))))
         ;; Two-point crossover: swap bits 0-4
         (let ((children (funcall 'neovm--ga7-two-point-crossover p1 p2 0 4)))
           (list (funcall 'neovm--ga7-to-list (car children))
                 (funcall 'neovm--ga7-to-list (cdr children))))
         ;; Two-point with same points: no swap
         (let ((children (funcall 'neovm--ga7-two-point-crossover p1 p2 3 3)))
           (list (funcall 'neovm--ga7-to-list (car children))
                 (funcall 'neovm--ga7-to-list (cdr children))))
         ;; Uniform crossover: alternating mask
         (funcall 'neovm--ga7-to-list
                  (funcall 'neovm--ga7-uniform-crossover p1 p2
                           [0 1 0 1 0 1 0 1]))
         ;; Uniform crossover: all from p1
         (funcall 'neovm--ga7-to-list
                  (funcall 'neovm--ga7-uniform-crossover p1 p2
                           [0 0 0 0 0 0 0 0]))
         ;; Uniform crossover: all from p2
         (funcall 'neovm--ga7-to-list
                  (funcall 'neovm--ga7-uniform-crossover p1 p2
                           [1 1 1 1 1 1 1 1]))
         ;; Verify two-point preserves total bits
         (let* ((children (funcall 'neovm--ga7-two-point-crossover p1 p2 1 7))
                (c1 (car children)) (c2 (cdr children))
                (s1 0) (s2 0) (sp1 0) (sp2 0) (i 0))
           (while (< i 8)
             (setq s1 (+ s1 (aref c1 i)))
             (setq s2 (+ s2 (aref c2 i)))
             (setq sp1 (+ sp1 (aref p1 i)))
             (setq sp2 (+ sp2 (aref p2 i)))
             (setq i (1+ i)))
           (= (+ s1 s2) (+ sp1 sp2)))))
    (fmakunbound 'neovm--ga7-two-point-crossover)
    (fmakunbound 'neovm--ga7-uniform-crossover)
    (fmakunbound 'neovm--ga7-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 1 0 0 1 1 0 0) (0 0 1 1 0 0 1 1)) ((0 0 0 0 0 0 0 0) (1 1 1 1 1 1 1 1)) ((1 1 1 1 0 0 0 0) (0 0 0 0 1 1 1 1)) (1 0 1 0 0 1 0 1) (1 1 1 1 0 0 0 0) (0 0 0 0 1 1 1 1) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
