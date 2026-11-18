// Independent references for the scientific kernels of Benchmark V2. Every RESULT line is compared with these values before
// its time is used. Each reference is computed here in JavaScript, separately from the Ostrin, Python and Julia code.
// Shared by run-v2.mjs and compare-scientific.mjs.

function reference(kernel) {
  switch (kernel) {
    case "sieve": {
      // Primes below 300000 by the sieve of Eratosthenes (pi(300000) = 25997).
      const n = 300000;
      const composite = new Uint8Array(n);
      let count = 0;
      for (let i = 2; i < n; i += 1) {
        if (!composite[i]) {
          count += 1;
          for (let j = i * i; j < n; j += i) composite[j] = 1;
        }
      }
      return { values: [count], tolerance: 0 };
    }
    case "collatz": {
      // Total Collatz steps for the starts 1 to 19999.
      let total = 0;
      for (let n = 1; n < 20000; n += 1) {
        let x = n;
        while (x !== 1) {
          x = x % 2 === 0 ? x / 2 : 3 * x + 1;
          total += 1;
        }
      }
      return { values: [total], tolerance: 0 };
    }
    case "simpson":
      return { values: [2.0], tolerance: 1e-12 };
    case "bisect": {
      let x = 2.0;
      for (let i = 0; i < 60; i += 1) x -= (x * x * x - 2 * x - 5) / (3 * x * x - 2);
      return { values: [x], tolerance: 1e-12 };
    }
    case "matmul": {
      const n = 300;
      const a = new Array(n * n);
      const b = new Array(n * n);
      for (let i = 0; i < n; i += 1) {
        for (let j = 0; j < n; j += 1) {
          a[i * n + j] = (i * j + i + 2 * j) % 7;
          b[i * n + j] = (3 * i + j * j) % 5;
        }
      }
      const c = new Array(n * n).fill(0);
      for (let i = 0; i < n; i += 1) {
        for (let j = 0; j < n; j += 1) {
          let acc = 0;
          for (let k = 0; k < n; k += 1) acc += a[i * n + k] * b[k * n + j];
          c[i * n + j] = acc;
        }
      }
      let trace = 0;
      let total = 0;
      for (let i = 0; i < n; i += 1) {
        trace += c[i * n + i];
        for (let j = 0; j < n; j += 1) total += c[i * n + j];
      }
      return { values: [trace, total], tolerance: 0 };
    }
    case "regression":
      return { values: [1, 2, -3], tolerance: 1e-9 };
    case "horner": {
      const degree = 50;
      const coefficients = [];
      for (let k = 0; k <= degree; k += 1) coefficients.push((k % 5) - 2);
      let total = 0;
      for (let i = 0; i < 2000000; i += 1) {
        const x = (i % 1000) / 1000;
        let value = 0;
        for (let k = 0; k <= degree; k += 1) value = value * x + coefficients[degree - k];
        total += value;
      }
      return { values: [total], tolerance: 1e-12 };
    }
    default:
      throw new Error(kernel);
  }
}

export { reference };
