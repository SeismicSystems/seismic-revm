contract SaddressBasic {
    saddress a;

    function test() public {
        // can set a using literal
        a = 0xF390C18585583033b69fEc03298fE18568C89A81;
        assert(a == 0xF390C18585583033b69fEc03298fE18568C89A81);

        // can set a using address;
        address b = address(1);
        a = b;
        assert(a == address(1));

        // can set a using saddress
        saddress c = address(2);
        a = c;
        assert(a == address(2));

        // casting
        a = address(4);
        address d = address(a);
        assert(d == address(4));

        a = saddress(5);
        assert(a == address(5));
    }
}