use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

pub struct TestLogger {
    pub test_module: String,
    pub test_name: String,
    pub log_file: fs::File,
}

impl TestLogger {
    /// Creates a new TestLogger for the given test name.
    pub fn new(test_module: &str, test_name: &str) -> io::Result<Self> {
        let logs_dir = Path::new("logs").join(test_module);
        if !logs_dir.exists() {
            fs::create_dir_all(logs_dir.clone())?;
        }

        let log_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true) // Truncate the file if it already exists
            .open(logs_dir.join(format!("{test_name}.txt")))?;

        let compute_units_file_path = logs_dir.join(format!("compute_units.json"));

        // Create the file if it doesn't exist, and initialize it with an empty JSON object
        if !compute_units_file_path.exists() {
            let mut file = fs::File::create(&compute_units_file_path)?;
            file.write_all(b"{}")?;
        }

        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(compute_units_file_path)?;

        let logger = TestLogger {
            test_module: test_module.to_string(),
            test_name: test_name.to_string(),
            log_file,
        };

        logger.clear_records_for_test()?;

        Ok(logger)
    }

    /// Writes contents to the test log file.
    pub fn write(&mut self, contents: &str) {
        self.log_file
            .write_all(format!("{contents}\n").as_bytes())
            .unwrap()
    }

    pub fn clear_records_for_test(&self) -> io::Result<()> {
        let log_dir = Path::new("logs").join(&self.test_module);
        let compute_units_file_path = log_dir.join(("compute_units.json"));

        // Read the existing JSON object from the file
        let mut file = fs::File::open(&compute_units_file_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        // Parse the JSON object
        let mut json_obj: Value = serde_json::from_str(&contents).unwrap_or_else(|_| json!({}));

        let array = json_obj
            .as_object_mut()
            .and_then(|obj| obj.get_mut(&self.test_name))
            .and_then(|val| val.as_array_mut());

        // if let Some(array) = array {
        //     // Append the new unit to the existing array
        //     array.push(json!());
        // } else {
        //     // Create a new array with the compute unit
        json_obj[&self.test_name] = json!([]);
        // }

        Ok(())
    }

    /// Records the compute units used and writes to the JSON object in the log file.
    pub fn record_compute(&self, units: u64) -> io::Result<()> {
        let log_dir = Path::new("logs").join(&self.test_module);
        let compute_units_file_path = log_dir.join(("compute_units.json"));

        // Read the existing JSON object from the file
        let mut file = fs::File::open(&compute_units_file_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        // Parse the JSON object
        let mut json_obj: Value = serde_json::from_str(&contents).unwrap_or_else(|_| json!({}));

        // Get the array for the test name, or create a new one
        let array = json_obj
            .as_object_mut()
            .and_then(|obj| obj.get_mut(&self.test_name))
            .and_then(|val| val.as_array_mut());

        if let Some(array) = array {
            // Append the new unit to the existing array
            array.push(json!(units));
        } else {
            // Create a new array with the compute unit
            json_obj[&self.test_name] = json!([units]);
        }

        // Write the updated JSON object back to the file
        let new_contents = serde_json::to_string_pretty(&json_obj)?;
        fs::write(&compute_units_file_path, new_contents)?;

        Ok(())
    }
}
