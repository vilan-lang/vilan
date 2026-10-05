main: {
	const clamp = (x) => {
		if (x > 1) {
			return 9;
		}
		return x;
	};
	console.log(String(clamp(0)));
	console.log(String(clamp(5)));
	let i = 0;
	while (i < 5) {
		if (i === 2) {
			break main;
		}
		console.log(String(i));
		i = i + 1;
	}
	console.log("not reached");
}
