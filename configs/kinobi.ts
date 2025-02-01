import path from 'path';
import {
  createFromRoot,
  pdaNode,
  addPdasVisitor,
  constantPdaSeedNodeFromString,
  variablePdaSeedNode,
  publicKeyTypeNode,
  numberTypeNode,
  updateInstructionsVisitor,
  pdaValueNode,
  setInstructionAccountDefaultValuesVisitor,
  updateAccountsVisitor,
  pdaLinkNode,
} from 'codama';
import { rootNodeFromAnchor } from '@codama/nodes-from-anchor';
import { renderJavaScriptVisitor, renderRustVisitor } from '@codama/renderers';

import dotenv from 'dotenv';
dotenv.config();

const clientDir = path.join(__dirname, '..', 'clients');
const programDir = path.join(__dirname, '..', 'programs', 'ballista');

const ballistaIdl = require(path.join(programDir, 'ballista.json'));

ballistaIdl.accounts = [
  {
    name: 'taskDefinition',
    data: [
      {
        name: 'id',
        type: 'u8',
      },
    ],
  },
];

// Instanciate Codama.
const codama = createFromRoot(rootNodeFromAnchor(ballistaIdl));

// // Memory account PDA
codama.update(
  addPdasVisitor({
    ballista: [
      pdaNode({
        name: 'taskDefinition',
        seeds: [
          constantPdaSeedNodeFromString('utf8', 'task-definition'),
          variablePdaSeedNode('payer', publicKeyTypeNode()),
          variablePdaSeedNode('taskId', numberTypeNode('u8')),
        ],
      }),
    ],
  }),
);

// codama.getRoot().program.pdas.push()

codama.update(
  setInstructionAccountDefaultValuesVisitor([
    // {
    //     // Set this public key as default value to any account named 'counterProgram'.
    //     account: 'counterProgram',
    //     defaultValue: publicKeyValueNode('MyCounterProgram11111111111111111111111111'),
    // },
    {
      // Set this PDA as default value to any account named 'associatedToken' or 'ata'.
      account: /^task/,
      defaultValue: pdaValueNode('taskDefinition'),
    },
  ]),
);

codama.update(
  updateAccountsVisitor({
    ballista: {
      name: 'taskDefinition',
      pda: pdaLinkNode('taskDefinition'),
      seeds: [
        constantPdaSeedNodeFromString('utf8', 'task-definition'),
        variablePdaSeedNode('payer', publicKeyTypeNode()),
        variablePdaSeedNode('id', numberTypeNode('u8')),
      ],
    },
  }),
);

// // How to set a default value for an account in an instruction.
codama.update(
  updateInstructionsVisitor({
    createTask: {
      accounts: {
        taskDefinition: {
          defaultValue: pdaValueNode('taskDefinition'),
        },
      },
    },
  }),
);

console.log(codama.getRoot());

// Render preview JavaScript.
const jsDir = path.join(clientDir, 'js', 'src', 'generated');
codama.accept(renderJavaScriptVisitor(jsDir, {}));

// Render Rust.
const crateDir = path.join(clientDir, 'rust');
const rustDir = path.join(clientDir, 'rust', 'src', 'generated');
codama.accept(
  renderRustVisitor(rustDir, {
    formatCode: true,
    crateFolder: crateDir,
    linkOverrides: {
      definedTypes: {
        value: 'hooked',
        expression: 'hooked',
        taskDefinition: 'hooked',
      },
    },
  }),
);
