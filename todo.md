### Priority

[] Task Definition Settings (Authority Only, Immutable)




















[ ] Build testing framework at accelerates development of ballista
---[ ] Task Definitions in common repo for SPL program
---[ ] Find some programs to include in testing (fork marginfi, etc)
---[ ] Move evaluation functions and task state into common so that can unit test execute task with evaluation functions
---[ ] IDL JSON to instruction schema.
---[ ] Output failing tests into individual log files and aggregate them.
---[ ] Record Compute Unit types in individual log files and aggregate them.
---[ ] Refactor test utils into sensible modules and reduce the cluster fuck.
---[ ] proc macro that takes a instruction schema and creates a task builder
------[ ] SystemTransferTaskBuilder::SetArguments([{name: string, argument: Value, validator: |V| : true | Err}; 3])
---------[ ] is type validator, validators are simulated in task execute.
------[ ] SystemTransferTaskBuilder::SetAccounts([{name: string, source: AccountSource, validator: |pubkey| : true | Err }; 2])
------[ ] SystemTransferTaskBuilder -> TaskExecutor
------[ ] TaskExecutor.set_arg("")

Ballista has a instructions for validating outcomes.

IDL -> this

MintV1Instruction {
discriminator: TaskArgument::Const(Value::Byte([...]))
owner: TaskArgument::Input,
metadata: TaskArgument::Input,
}


