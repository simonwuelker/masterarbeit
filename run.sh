#!/bin/bash

spack env activate masterarbeit

timeout 6000 cargo r -r -- server --mallob=../mallob2 --problem-directory=../mallob/problems --temp-directory=/nfs/scratch/swuelker/temp

status=$?
if [[ $status -eq 124 ]]; then
  echo "Command timed out after 1000 seconds."
  exit 124
else
  echo "Command completed without timing out."
  exit "$status"
fi
