#!/bin/bash

spack env activate masterarbeit

cargo r -r -- server --mallob=../mallob --problem-directory=../mallob/problems --temp-directory=temp
