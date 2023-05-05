let content = open_file("secretfile.txt", "r").read_string();

content += "\n\n";

let number = 50;

while number > 0 {
    content += number + " ";
    number -= 1;
}

print("pagman");

return content;