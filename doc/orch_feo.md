# Orchestration & FEO

## Static view

![Alt text](static_view.drawio.svg)

## FEO agents
FEO agents are responsible to wait on `Executor` commands and execute them. Below we describe how each part of FEO agent is transcoded by `Orchestration API`.

### Startup (A)
Each FEO agent on startup is suppose to:
- let primary process know it's alive
- wait for startup signal
- report started and enter into normal execution (waiting for orders)

This is encoded by using combination of `Sequence` and `Concurrency` `actions`:

```mermaid
graph LR;
    Trigger["Trigger(application_id/agent_id/alive)"]-->SyncS["Sync(application_id/startup)"]

    SyncS --> Activity1["Activity1.startup()"];
    SyncS --> Activity2["Activity2.startup()"];
    SyncS --> Activity3["Activity3.startup()"];

    Activity1 --> TriggerE["Trigger(application_id/agent_id/startup_completed)"]
    Activity2 -->TriggerE;
    Activity3 -->TriggerE;

    TriggerE --> SyncSS["Sync(application_id/start)"]

    subgraph Concurrency
        Activity1
        Activity2
        Activity3
    end

    subgraph Sequence
        SyncS
        TriggerE
        Concurrency
        Trigger
        SyncSS
    end

```

### Step (B)
Each activity step function is encoded by simple `Sequence` of `actions`:

```mermaid
graph LR;
    Sync["Sync(application_id/activity_id/step)"]-->Step;
    Step["ActivityN.step()"]--> Trigger["Sync(application_id/activity_id/step_completed)"];

    subgraph Sequence
        Sync
        Step
        Trigger
    end
```

### Shutdown (C)

The shutdown functionality is encoded by simple `Sequence` and `Concurrency` `actions`:

```mermaid
graph LR;

    Activity1["Activity1.shutdown()"];
    Activity2["Activity2.shutdown()"];
    Activity3["Activity3.shutdown()"];

    Activity1 --> TriggerE["Trigger(application_id/agent_id/shutdown_completed)"]
    Activity2 -->TriggerE;
    Activity3 -->TriggerE;

    subgraph Concurrency
        Activity1
        Activity2
        Activity3
    end

      subgraph Sequence
        SyncS
        TriggerE
        Concurrency
    end

```

### Shutdown request (D)
The agent shall all time wait for a request to shutdown, this is relized simply by registering hook:

```mermaid
graph LR;
    Sync["Sync(application_id/shutdown)"]
```

### Overall FEO agent 
The overall FEO agent implemented by `Orchestration API` is a `Program` that is composed using above functionalities as below:

![Alt text](feo_agent.png)



## FEO Executor
The executor is responsible for
- coordinate startup
- executing graph logic for activities
- coordinate shutdown
- ....

### Startup (A)
The startup coordination is build as below:

```mermaid
graph LR;
    SyncA1["Sync(application_id/agent_id_1/alive)"];
    SyncA2["Sync(application_id/agent_id_2/alive)"];
    SyncA3["Sync(application_id/agent_id_2/alive)"];

    SyncA1 --> TriggerS;
    SyncA2 --> TriggerS;
    SyncA3 --> TriggerS;

    TriggerS["TriggerA1(application_id/startup)"];

    TriggerS --> SyncA1C;
    TriggerS --> SyncA2C;
    TriggerS --> SyncA3C;


    SyncA1C["Sync(application_id/agent_id_1/startup_completed)"];
    SyncA2C["Sync(application_id/agent_id_2/startup_completed)"];
    SyncA3C["Sync(application_id/agent_id_2/startup_completed)"];

    TriggerSE["TriggerA1(application_id/start)"];

    SyncA1C --> TriggerSE;
    SyncA2C --> TriggerSE;
    SyncA3C --> TriggerSE;

    subgraph Concurrency2
        SyncA1C
        SyncA2C
        SyncA3C
    end

    subgraph Concurrency1
        SyncA1
        SyncA2
        SyncA3
    end

    subgraph Sequence
        Concurrency1
        TriggerS
        Concurrency2
        TriggerSE
    end
```

### Graph execution (B)
The graph of activities is translated currently with below schema:
- each activity is separate concurrent branch that can run as soon as all conditions are met
- each activity without dependency can run immediately in single cycle
- each activity with dependencies is translated into `Sequence` of `Sync` actions for corresponding "done" event before it can run
- each run execution is encoded as `Trigger` (start) & `Sync` (wait for finish) action


The above bolis down to translate below graph:

```mermaid
graph TD;
0 --> 2;
1 --> 2;

2 --> 3;

2 --> 4;
2 --> 5;

4 --> 6;
5 --> 7;
```

into 

```mermaid
graph TD;

    %% Activity 0
    TriggerA0E["Trigger(application_id/0/step)"];
    SyncA0EC["Sync(application_id/0/step_completed)"];


    TriggerA0E --> SyncA0EC;

    subgraph Sequence0
    direction TB
       TriggerA0E
       SyncA0EC
    end

    %% Activity 1
    TriggerA1E["Trigger(application_id/1/step)"];
    SyncA1EC["Sync(application_id/1/step_completed)"];


    TriggerA1E --> SyncA1EC;

    subgraph Sequence1
        direction TB
        TriggerA1E
        SyncA1EC
    end

  

    %% Activity 2

    SyncA2D1["Sync(application_id/0/step_completed)"];
    SyncA2D2["Sync(application_id/1/step_completed)"];

    TriggerA2E["Trigger(application_id/2/step)"];
    SyncA2EC["Sync(application_id/2/step_completed)"];

    SyncA2D1 --> SyncA2D2;
    SyncA2D2 --> TriggerA2E;
    TriggerA2E --> SyncA2EC;

    subgraph Sequence2
    direction TB
        SyncA2D1
        SyncA2D2
        TriggerA2E
        SyncA2EC
    end

     %% Activity 3

    SyncA3D1["Sync(application_id/2/step_completed)"];

    TriggerA3E["Trigger(application_id/3/step)"];
    SyncA3EC["Sync(application_id/3/step_completed)"];


    SyncA3D1 --> TriggerA3E;
    TriggerA3E --> SyncA3EC;

    subgraph Sequence3
    direction TB
        SyncA3D1
        TriggerA3E
        SyncA3EC
    end

 %% Activity 4

    SyncA4D1["Sync(application_id/2/step_completed)"];

    TriggerA4E["Trigger(application_id/4/step)"];
    SyncA4EC["Sync(application_id/4/step_completed)"];


    SyncA4D1 --> TriggerA4E;
    TriggerA4E --> SyncA4EC;

    subgraph Sequence4
    direction TB
        SyncA4D1
        TriggerA4E
        SyncA4EC
    end

     %% Activity 5

    SyncA5D1["Sync(application_id/2/step_completed)"];

    TriggerA5E["Trigger(application_id/5/step)"];
    SyncA5EC["Sync(application_id/5/step_completed)"];


    SyncA5D1 --> TriggerA5E;
    TriggerA5E --> SyncA5EC;

    subgraph Sequence5
    direction TB
        SyncA5D1
        TriggerA5E
        SyncA5EC
    end

     %% Activity 6

    SyncA6D1["Sync(application_id/4/step_completed)"];

    TriggerA6E["Trigger(application_id/6/step)"];
    SyncA6EC["Sync(application_id/6/step_completed)"];


    SyncA6D1 --> TriggerA6E;
    TriggerA6E --> SyncA6EC;

    subgraph Sequence6
    direction TB
        SyncA6D1
        TriggerA6E
        SyncA6EC
    end

     %% Activity 7

    SyncA7D1["Sync(application_id/5/step_completed)"];

    TriggerA7E["Trigger(application_id/7/step)"];
    SyncA7EC["Sync(application_id/7/step_completed)"];


    SyncA7D1 --> TriggerA7E;
    TriggerA7E --> SyncA7EC;

    subgraph Sequence7
    direction TB
        SyncA7D1
        TriggerA7E
        SyncA7EC
    end


    subgraph Concurrency
       Sequence0
       Sequence1
       Sequence2
       Sequence3
       Sequence4
       Sequence5
       Sequence6
       Sequence7
    end

```

### Shutdown (C)
The shutdown coordination is build as below:

```mermaid
graph LR;

    ExecS["Trigger(application_id/shutdown)"] --> Sync1["Sync(application_id/agent_1_id/shutdown_completed)"];

    ExecS --> Sync2["Sync(application_id/agent_2_id/shutdown_completed)"];
    ExecS --> Sync3["Sync(application_id/agent_3_id/shutdown_completed)"];

    subgraph Sequence
        ExecS
        Concurrency
    end

    subgraph Concurrency
        Sync1
        Sync2
        Sync3
    end

```


### Shutdown request (D)

The primary process can connect any source to start shutdown routine. Currently this is not used in FEO as such signals is not defined.


### Overall FEO executor 
The overall FEO executor implemented by `Orchestration API` is a `Program` that is composed using above functionalities as below:

![Alt text](feo_executor.png)

Additionally `ProgramBuilder` let us configure:
- cycle time
- overall error reaction
- ...