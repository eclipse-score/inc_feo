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
      depends_on: # List of dependencies
        - *GetData_label
    RunConcurrent2: &RunConcurrent2_label
      prop1: 1
      depends_on:
        - *RunConcurrent2_label
    RunSync: &RunSync_label
      prop1: 3
      depends_on:
        - *RunSync_label


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
        depends_on:
          - *Activity1_label
      Activity3: &Activity3_label
        depends_on:
          - *Activity1_label
      Activity4: &Activity4_label
        depends_on:
          - *Activity2_label
      Activity5: &Activity5_label
        depends_on:
          - *Activity3_label
      Activity6: &Activity6_label
        depends_on:
          - *Activity5_label
      Activity7: &Activity7_label
        depends_on:
          - *Activity5_label
      Activity8: &Activity8_label
        depends_on:
          - *Activity5_label
