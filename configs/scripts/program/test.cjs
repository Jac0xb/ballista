const { execSync } = require('child_process');
const { spawn } = require('child_process');
const path = require('path');

// run .env file
require('dotenv').config();

const WORKING_DIR = process.cwd();

process.env.BPF_OUT_DIR = path.join(WORKING_DIR, 'configs', '.programs');

console.log('BPF_OUT_DIR:', process.env.BPF_OUT_DIR);

// Function to execute a command and log output
function runCommand(command, args = []) {
  console.log('Executing command:', command, args.join(' '));
  return new Promise((resolve, reject) => {
    const process = spawn(command, args, {
      stdio: 'inherit', // This will pipe output directly to console
      shell: true,
    });

    process.on('close', (code) => {
      if (code !== 0) {
        reject(new Error(`Command failed with exit code ${code}`));
      } else {
        resolve();
      }
    });

    process.on('error', (err) => {
      console.error('Failed to start subprocess:', err);
      reject(err);
    });
  });
}

// Change to working directory
process.chdir(WORKING_DIR);

// Process command-line arguments
let args = process.argv.slice(2);
let programs;
if (args.length > 0) {
  programs = `["${args[0]}"]`;
  args = args.slice(1);
}

async function main() {
  // Change to programs/ballista directory and run tests
  const ballistaPath = path.join(WORKING_DIR, 'programs', 'ballista');
  process.chdir(ballistaPath);

  console.log('Changing directory to: ', ballistaPath);
  console.log(`Running solana test-sbf for ballista...`);
  await runCommand('cargo', ['test']);

  const commonPath = path.join(WORKING_DIR, 'common');
  process.chdir(commonPath);

  console.log('Changing directory to: ', commonPath);
  console.log(`Running cargo test for common...`);
  await runCommand('cargo', ['test']);
}

main().catch((error) => {
  console.error('Test execution failed:', error);
  process.exit(1);
});

// // Run tests
// process.chdir(path.join(WORKING_DIR, 'tests', 'ballista'));
// try {
//   execSync('cargo test', {
//     env: {
//       ...process.env,
//       RUST_LOG: 'error',
//       RUST_BACKTRACE: '1',
//       SOLFMT: 'solfmt',
//       SBF_OUT_DIR: path.join(WORKING_DIR, process.env.PROGRAMS_OUTPUT),
//     },
//     stdio: 'inherit',
//   });
// } catch (error) {
//   console.error('Test execution failed:', error);
//   process.exit(1);
// }
