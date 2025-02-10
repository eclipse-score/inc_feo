# FEO execution environment

<!-- cargo-rdme start -->

FEO is an application framework for data- and time-driven applications in the ADAS domain.
The name is an abbreviation of Fixed Execution Order.

## Activities and Task Chains

[Activities](https://docs.rs/feo/latest/feo/activity/trait.Activity.html) are the units of computation. This could be an algorithm which detects and extracts
lane information from a provided camera image. Such activities are the building blocks of a
task chain which is executed cyclically.

## Communication via Topics

Data exchange between activities is provided by [feo::com](https://docs.rs/feo/latest/feo/com/). Each activity can be configured
to read and write messages to a named topic.

## Execution of Activities

A FEO application consist of one or more agents (processes) with one or more workers (threads)
per agent.
Each activity is statically mapped to one agent and one worker through [feo::configuration](https://docs.rs/feo/latest/feo/configuration/).

<!-- cargo-rdme end -->
