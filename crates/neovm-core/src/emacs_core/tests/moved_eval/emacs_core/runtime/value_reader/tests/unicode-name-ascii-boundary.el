(mapcar (lambda (name)
  (list
    (condition-case e (car (read-from-string (concat "?\\N{" name "}"))) (error e))
    (condition-case e (car (read-from-string (concat "\"\\N{" name "}\""))) (error e))))
  '("LATIN SMALL LETTER LONG ſ" "LATIN SMALL LETTER LONG ß"))
