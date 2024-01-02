#!/usr/bin/bash

rsync -rtlpP --exclude=.git/** --exclude=target/** /home/agerasev/develop/my/rtherm/ 10.4.0.10:develop/rtherm
