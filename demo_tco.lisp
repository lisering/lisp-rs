;; ==========================================
;; TCO 对比演示
;; ==========================================

;; 1. 尾递归版本（tail-recursive）
;;    递归调用在 if 的尾位置，触发 TCO
(display "=== 尾递归版本（享受 TCO）===")
(newline)
(define loop (lambda (n)
  (if (= n 0)
      "done"
      (loop (- n 1)))))

(display "  调用 (loop 1000000) ...")
(newline)
(display "  结果: ")
(display (loop 1000000))
(newline)
(newline)

;; 2. 非尾递归版本（non-tail-recursive）
;;    递归调用在 + 的参数位置，不在尾位置，不触发 TCO
(display "=== 非尾递归版本（不享受 TCO）===")
(newline)
(define sum (lambda (n)
  (if (= n 0)
      0
      (+ n (sum (- n 1))))))

(display "  调用 (sum 500) ...")
(newline)
(display "  结果: ")
(display (sum 500))
(newline)
(newline)

(display "  调用 (sum 10000) ...")
(newline)
(display "  结果: ")
(display (sum 10000))
(newline)
(newline)

(display "=== 结论 ===")
(newline)
(display "  尾递归   (loop 1000000): 成功 — TCO 把递归变成了循环")
(newline)
(display "  非尾递归 (sum 10000):    栈溢出 — 每次调用都要等待 (+ n ...) 的结果")
(newline)
