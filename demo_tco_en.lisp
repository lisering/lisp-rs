;; ==========================================
;; TCO Demo
;; ==========================================

;; 1. Tail-recursive version
;;    Recursive call is in tail position of `if`, triggers TCO
(display "=== Tail-recursive (with TCO) ===")
(newline)
(define loop (lambda (n)
  (if (= n 0)
      "done"
      (loop (- n 1)))))

(display "  Calling (loop 1000000) ...")
(newline)
(display "  Result: ")
(display (loop 1000000))
(newline)
(newline)

;; 2. Non-tail-recursive version
;;    Recursive call is an argument to +, NOT in tail position, no TCO
(display "=== Non-tail-recursive (no TCO) ===")
(newline)
(define sum (lambda (n)
  (if (= n 0)
      0
      (+ n (sum (- n 1))))))

(display "  Calling (sum 500) ...")
(newline)
(display "  Result: ")
(display (sum 500))
(newline)
(newline)

(display "  Calling (sum 10000) ...")
(newline)
(display "  Result: ")
(display (sum 10000))
(newline)
(newline)

(display "=== Conclusion ===")
(newline)
(display "  Tail-recursive  (loop 1000000): Success — TCO turned recursion into a loop")
(newline)
(display "  Non-tail-rec    (sum 10000):    Stack overflow — each call waits for (+ n ...)")
(newline)
