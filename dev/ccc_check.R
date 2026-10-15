d <- read.csv("tests/test_1_in.csv")
q <- qnorm(0.975)
res <- do.call(rbind, lapply(split(d, list(d$sp, d$variable)), function(g) {
  x <- g$x_values; y <- g$y_values; n <- length(x)
  mx <- mean(x); my <- mean(y)
  sxx <- mean((x-mx)^2); syy <- mean((y-my)^2); sxy <- mean((x-mx)*(y-my))
  rc <- 2*sxy/(sxx+syy+(mx-my)^2); r <- sxy/sqrt(sxx*syy); u <- (mx-my)/(sxx*syy)^0.25
  z <- atanh(rc)
  se <- sqrt(((1-r^2)*rc^2/((1-rc^2)*r^2) + 2*rc^3*(1-rc)*u^2/(r*(1-rc^2)^2) - rc^4*u^4/(2*r^2*(1-rc^2)^2))/(n-2))
  data.frame(sp=g$sp[1], variable=g$variable[1], ccc=rc, lo=tanh(z-q*se), hi=tanh(z+q*se), r=r)
}))
write.csv(res, "tests/r_crosscheck.csv", row.names=FALSE)
print(res, digits=15)
