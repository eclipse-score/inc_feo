Component architecture: Fixed execution order framework
=======================================================

See [Contribution Request Guideline](https://eclipse-score.github.io/score/process/guidance/contribution_request/index.html) and [Feature Request Template](https://eclipse-score.github.io/score/process/guidance/contribution_request/templates/feature_request_template.html).

General
-------

* FEO shall be a framework for applications
* For data-driven and time-driven applications (mainly in the ADAS domain)
* Supporting fixed execution order
* Supporting reprocessing 

Applications
------------

* The framework is used to build applications
* Multiple applications based on the framework can run in parallel on the same host machine
* Applications based on the framework can run in parallel with other applications not based on the framework
* The framework does not support communication between different applications (except via service activities, see below)

Activities
----------

* Applications consist of activities
* Activities are a means to structure applications into building blocks
* Activities have init(), step() and shutdown() entry points
* The framework provides the following APIs to the activities running on it:
  - Read time (feo::time)
  - Communicate to other activities (feo::com)
  - Log (feo::log)
  - Configuration parameters (feo::param)
  - Persistency (feo::pers)
* There are two types of activities:
  - Application activities
  - Service activities
* Application activities must only use APIs provided by the framework as defined above
* Application activities are single threaded, they can not run outside of their entry points, 
  they must not spawn other threads or processs
* Activities can be implemented in C++ or Rust, mixed systems with both
  C++ and Rust activities are possible


Service Activities
------------------

* Service activities are a means to interact with the outside world, e.g. via
  network communication, direct sensor input or direct actuator output
* Service activities may also use APIs external to the framework
  (e.g. networking APIs, reading from external sensor devices, writing HW I/O, etc.)
* Service activities run at the beginning ("input service activity") and at the end 
  ("output service activity") of a tash chain (see below)
* Input service activities provide the input values to the application activities 
  within the task chain, by means of communication
* All input service activities must finish execution before the first application activity
  is run. this can be achieved by proper setup of the chain dependencies (see below)
* There must be at least one input service activity
* Output service activities consume output values from the application activities
  calculated within the task chain an provide them to the outside world
* All output service activities must run after all application service activities have
  finished execution. this is achieved by proper setup of the chain dependencies (see below)
* There must be at least one output service activity


Communication
-------------

* Application type activities can only communicate to other activities within 
  the same application and using the provided communication API
* Communication consists of sending and receiving messages on named topics
* The receiver of a message on a topic does not know the sender, instead it only
  relies on the message itself independent of the source of the message
* There can only be one sender per topic but multiple receivers
* Optional: there can be multiple senders per topic
* There is no publish/subscribe mechanism acessible to activities, instead
  the set of known communication topics and the assignment of which activity
  sends and receives to/from which topic is "runtime static"
* "runtime static" means "static after the startup phase", i.e. during startup, the 
  framework can configure or build up communication connections, but as soon as the
  run phase starts (where the activties' step() functions are called), the connections
  are fixed and will not change any more.
* Communication relations are typically configured in configuration files
* Messages/topics are statically typed
* Only messages of the matching type can be sent/received on a specific topic
* The binary representation of messages is defined by the framework in order
  to support communication between activities implemented in different
  languages (C++/Rust)
* Message types may be primitive types or complex (nested) types
* Complex types can be built by using structs and arrays of types
* Sending a message by an activity involves the following steps:
  - Call API to acquire a handle to a message buffer for a certain topic
  - Fill data into the provided memory buffer
  - Call API to send the message
* Reception of a message by an activity involves the following steps:
  - Use API to receive message from a certain topic, this returns a handle to a data buffer
  - Read message data from data buffer
* The receiver can not modify the message, the framework will enforce this,
  for example by using read-only types or by configuring memory protect of the OS

Queuing:
* Queuing can be enabled per topic, a queue of length N means that the last
  N messages are kept for a specific topic
* Receivers have access to the last N elements, reading an element from the
  queue by a receiver doesn't change the queue, i.e. doesn't remove it from the queue.
  instead all receiver will always see the last N elements
* Optional: a queue pointer to the element last read is maintained per receiver.
  however, the queue with its buffers still only exists once per topic. if one receiver
  receives an element from the queue, its queue pointer is incremented so that next
  time it reads the next element, this does not affect the queue pointers of other receivers
* Queue enable and queue length are "runtime static" configuration settings


Process/Thread Mapping
----------------------

* An application consists of one or more processes
* One of the processes is the primary process
* If there is more than one process, the other processes are secondary processes
* There can be one or more threads per process
* The number of processes and threads is statically defined and
  does not change once the application has been started (runtime static)
* Activities are statically mapped to threads within processes within the application
* There can be multiple activities mapped to the same thread

* There is one executable per process, so an application may consist of multiple executables
* Each executable contains part of this framework as well as the activities mapped to the
  corresponding process
* It is assumed that an external entity starts all the executables belonging to the 
  same application. the reason for this is that for security reasons, only very
  specific entities should have the ability to create processes
* The executables belonging to an application are grouped (e.g. in the filesystem) so that
  it's clear that they belong together
* One reason for having multiple processes per application is to 
  achieve Freedom From Interference for safety relevant applications


Lifecycle
---------

* The lifecycle of an application consists of 3 phases:
  - startup phase
  - run phase
  - shutdown phase
* During startup phase, the primary proces connects with the secondary processes 
  (if present), in order to:
  - Build up connections for communication (e.g. find shared memory segments
    provided/consumed)
  - Connect to the parameter service
  - Coordinate the init and later the shutdown process
  - Coordinate the execution of the task chain (see below)
* During the shutdown phase, the primary process coordinates the shutdown of
  all secondary processes
* The connection between primary and secondary processes is kept up as long as the
  application is running
* If the connection breaks down unexpectedly while the application is running,
  the involved processes terminate (either by a command from the primary process
  or by detecting connection loss to the primary process)

Activity Init:
* At the end of the startup phase, the framework will invoke the init() entry point 
  of each activity
* The init() entry point will be invoked in the thread the activity is mapped to
* The order of invoking the init() entry points across activities is not defined,
  invocation may happen in parallel or sequentially

Activity Shutdown:
* At the beginning of the shutdown phase, the framework will invoke the shutdown() 
  entry point of each application
* The shutdown() entry point will be invoked in the thread the activity is mapped to
* The order of invoking the shutdown() entry points across activities is not defined,
  invocation may happen in parallel or sequentially


Scheduling
----------

* Activities are arranged in a task chain
* There is exactly one task chain per application
* The task chain describes the execution order of the activities in the run phase
* Task chains run cyclically, e.g. every 30ms
* Optional: task chains can be triggerd on event
* All activities are executed once per task chain run
* All activities finish within a single task chain run
* Running an activity means that the framework is calling its step() function 
  within the process/thread it has been mapped to
* The execution order is defined by a dependency model:
  - Each activity can depend on N other activities in the same task chain
  - An activity's step() function gets called as soon as the step() 
    functions of the activities it depends on have been called
* The framework takes care to run the activities in this order,
  independent of the thread/process the activity is mapped to
* While the order is guaranteed, there is no guarantee that an activity is
  run immediately after all its dependencies have finished.
  for example if two activities mapped to the same thread are ready to run
  at the same time, they can still only run one after the other
* Note however, that for a particular (static) setup of threads, processes
  and activity mapping, the invocation delay is deterministic
  (apart from differences in the activity execution times)
* The execution order and the exact point in time when an activity is run
  is independent of any communication an activity might do
* The dependencies should be defined by the application developer in a way so that 
  processing results passed via communication are available when they are needed
  (if an activity needs an output of another activity it sets that other
  activity as its dependency and therefore will only run once the other one
  is finished and therefore has produced the results the first one needs)


Executor and Agents
--------------------

* The coordinating entity in the primary process is the "executor"
* The executor coordinates the invocation of the activities in the
  order as described above
* As a central entity the executor is able to trace, record or monitor the 
  system behavior as sequence of activity invocations (see below)
* The actual activity invocation is done by an "agent"
* The agent exists in each process belonging to an application
* The agent connects to the executor during the startup phase
* The agent take invocation commands sent by the executor and
  executes them in its local process on behalf of the executor


External state
--------------

* Depending on the reprocessing scenario (see below) it might be necessary
  to put the activities into a well defined state. This can either be done
  by providing all the input to the activities which they need to get
  into that state (which could involve many task chain invocations).
  another way is to let the framework record activity state just as it 
  records communication messages
* External state is a means to make activity state recordable
* Using external state, activities don't hold their state in activity local
  variables (like C++ member variables) but in a state storage provided
  by the framework. this way, they "do not remember anything" from the
  last task chain invocation. instead, on every new task chain invocation,
  they first read in the external state from the framework provided storage,
  then potentially manipulate the state based on their inputs and then
  store it back for the next task chain invocation


Tracing
-------

* The framework can record all messages going over its communication topics
* For each message the recording includes:
  - topic
  - data
  - timestamp
  - sender [optional]
* The framework can record certain execution events:
  - task chain start/end
  - init/step/shutdown() entry point enter per activity
  - init/step/shutdown() entry point leave per activity
* For each event the recording includes:
  - type (e.g. step_enter)
  - context (e.g. activity name of step() entered)
  - timestamp


Reprocessing
------------

* There are multiple possible reprocessing scenarios, for example:
  - replay of one or many executions of a task chain
  - replay of one or many executions of a single activity
* In a replay scenario, the framework is used to reproduce the communication messages
  and other API behavior (e.g. time, parameters, persistency) as was 
  recorded in a previous run
* In case a whole task chain is reprocessed, the outputs of the input service activites
  will be reproduced
* In case only a single activity is reprocessed, the outputs of the predecessors
  in the task chain will be reproduced
* Outputs of application activities are typically not replayed but
  freshly calculated by the activities running during the replay
* The framework supports reprocessing by
  - Starting a task chain at the same point in time as recorded
  - Replaying communication data as recorded
  - Providing time via its time API as recorded


