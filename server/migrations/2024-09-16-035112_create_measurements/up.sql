-- Your SQL goes here
CREATE TABLE `measurements`(
	`channel` VARCHAR NOT NULL,
	`value` FLOAT NOT NULL,
	`time` TIME NOT NULL,
	PRIMARY KEY(`channel`, `time`)
);

