/**
 * Author a template in TypeScript: a SOL transfer whose amount is a run input.
 *
 * The compiled bytes are byte-identical to what the Rust builder produces for the same template
 * (see `clients/rust/examples/author_template.rs`) and to `fixtures/system-transfer.hex`.
 */
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  defineTemplate,
  expression,
  planTemplateUpload,
  systemTransfer,
} from '../src/index.js';

export const transfer = defineTemplate({
  inputs: { lamports: { type: 'u64' } },
  accounts: {
    // Programs must pin their address so callers cannot substitute another executable.
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    sender: { signer: true, writable: true },
    recipient: { writable: true },
  },
  steps: [
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('sender'),
      to: account.fixed('recipient'),
      lamports: expression.input('lamports'),
      label: 'paySender',
    }),
  ],
});

export const compiledTransfer = compileTemplate(transfer);
export const uploadPlan = planTemplateUpload(compiledTransfer, 0);

if (process.argv[1]?.endsWith('transfer.ts')) {
  console.log(`payload bytes: ${compiledTransfer.bytes.length}`);
  console.log(`sha256: ${Buffer.from(compiledTransfer.hash).toString('hex')}`);
  console.log(`upload: ${uploadPlan.mode} in ${uploadPlan.instructions.length} instruction(s)`);
  console.log(`source map: ${JSON.stringify(compiledTransfer.sourceMap)}`);
}
