// SPDX-License-Identifier: Apache-2.0
// Asking in the console: a yes or no, and a password shown only as stars while it is typed.
import { createInterface } from "node:readline";

export function ask(question) {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  return new Promise((done) => rl.question(question, (answer) => { rl.close(); done(answer.trim()); }));
}

export function askSecret(question) {
  return new Promise((done, fail) => {
    const input = process.stdin;
    process.stdout.write(question);
    if (!input.isTTY) {
      // Not a console (a test or a pipe): read one line as it is.
      const rl = createInterface({ input });
      rl.once("line", (line) => { rl.close(); done(line); });
      return;
    }
    let typed = "";
    input.setRawMode(true);
    input.resume();
    input.setEncoding("utf8");
    const onData = (chunk) => {
      for (const ch of chunk) {
        if (ch === "\r" || ch === "\n") {
          // An Enter left over from starting the command (or pressed before typing) is not a password: keep waiting.
          if (typed.length === 0) continue;
          input.setRawMode(false);
          input.pause();
          input.off("data", onData);
          process.stdout.write("\n");
          return done(typed);
        }
        if (ch === "\u0003") {
          input.setRawMode(false);
          process.stdout.write("\n");
          return fail(new Error("Stopped."));
        }
        // Each character shows as a star, so you can see that typing is landing and how much, without the password showing.
        if (ch === "\u0008" || ch === "\u007f") {
          if (typed.length > 0) {
            typed = typed.slice(0, -1);
            process.stdout.write("\b \b");
          }
        } else if (ch >= " ") {
          typed += ch;
          process.stdout.write("*");
        }
      }
    };
    input.on("data", onData);
  });
}
