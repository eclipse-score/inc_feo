Config file type - YML
----------------------
.. warning::
    This chapter only describes rationale, but is nothing to decide about config file type. Whether later it will go with YML/JSON, it's future discussion and we shall not focus on it during review.

============
Rationale
============
YAML is more user-friendly than JSON for manual editing and reading. It supports various features, including labels and references, which aid in the reduction of redundant entries, resulting in a more concise, overall file structure. 
Furthermore, YAML is fully compatible with JSON and can be converted into JSON format without any significant effort, if needed.

Config file schema
-------------------

.. code-block:: yml
   activities: # Define activities in the application
    GetData: &GetData_label
      whatever_property: 1 # Example how additional properties for the activities can be provided later on
    RunConcurrent1: &RunConcurrent1_label
      prop1: 2
    RunConcurrent2: &RunConcurrent2_label
      prop1: 1
    RunSync: &RunSync_label
      prop1: 3

  state: # Define any states needed for the switch action
    state1: &state1_label
      intitial_value: 1

  events: 3 Define events if needed
    event1: &event1_label

  # We assume that each flow is executed as a loop, running sequentially from the top to the bottom, unless it is shut down.
  flows: 
    flow_name1: # Flow declaration
      some_other_prop: whatever # Example how any new properties can be added to flow later
      actions: 
        - class: execute
          activity: *GetData_label
        - class: concurrent
          actions:
            - class: execute
              activity: *RunConcurrent1_label
            - class: execute
              activity: *RunConcurrent2_label
            - class: sequence
              actions:
                - class: execute
                  activity: *RunConcurrent1_label
                - class: execute
                  activity: *RunConcurrent2_label
                - class: trigger
                  event: *event1_label
        - class: execute
          activity: *RunSync_label
        - class: switch
          state: *state1_label
          default_action: # optional
            class: execute
            activity: *RunConcurrent2_label
          cases:
            - state_value: 0
              action:
                class: sequence
                actions:
                  - class: execute
                    activity: *RunConcurrent2_label
            - state_value: 2
              action:
                class: execute
                activity: *RunConcurrent2_label

    ## Included here for now to help understand what kinds of flow items are available.
    documentation_node:
      class:
        - sequence # organize activities to be called one after another
        - execute # activity execution, in particular calling Step()
        - concurrent # organize activities to be called concurrently
        - switch # execute activities based on a state
        - synchronize # wait for an event to continue
        - trigger # trigger and event


Example
-----------
Below simple example to illustrate modeled flow in design configuration file.

============
Flow description
============

.. image:: images/config_example.drawio.svg

============
Config
============
.. code-block:: yml
	activities: # Define activities in the application
      Activity1: &Activity1_label
      Activity2: &Activity2_label
      Activity3: &Activity3_label
      Activity4: &Activity4_label
      Activity5: &Activity5_label
      Activity6: &Activity6_label
      Activity7: &Activity7_label
      Activity8: &Activity8_label
    flows: 
      app_flow:
        actions: # PICTURE_1_TAG
          - class: execute
            activity: *Activity1_label
          - class: concurrent # PICTURE_2_TAG
            actions:
              - class: sequence # PICTURE_3_TAG
                actions:
                  - class: execute
                    activity: *Activity2_label
                  - class: execute
                    activity: *Activity4_label
              - class: sequence # PICTURE_4_TAG
                actions:
                  - class: execute
                    activity: *Activity3_label
                  - class: execute
                    activity: *Activity5_label
                  - class: concurrent
                    actions:
                      - class: execute
                        activity: *Activity6_label
                      - class: execute
                        activity: *Activity7_label
                       - class: execute
                        activity: *Activity8_label


To be done
-----------
When conclusion is reached, still to be done:

- schema (for sake of correct understanding in future)
- additional docs for config fields/sections